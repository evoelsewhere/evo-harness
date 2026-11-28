import * as React from "react";
import { ExternalLinkIcon, KeyRoundIcon, ZapIcon } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { appsApi, waysFor, type OfficialServer, type Toolkit } from "@/lib/connected-apps";
import type { Connector } from "@/lib/mcp";
import { cn } from "@/lib/utils";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";
import { useConnectorsStore } from "@/stores/connectors-store";
import { ConnectDialog, ServiceTile } from "./apps-parts";

// Official MCP servers and Zapier beside Composio (docs/features/
// connected-apps/spec.md, FR-20 – FR-24, FR-26): their marks, the dialog that
// offers a service's ways (official first), the token dialog, the Zapier
// card, and the pages of a connected official server or Zapier.

const open = (url: string) => void openUrl(url).catch(() => undefined);

/** "Official": the service's own server, direct and free. */
export function OfficialBadge({ className }: { className?: string }) {
  return <span className={cn("inline-flex h-5 items-center rounded-md bg-brand/12 px-1.5 text-[11px] text-brand", className)}>Official</span>;
}

export function PreviewBadge() {
  return <span className="inline-flex h-5 items-center rounded-md bg-amber-500/12 px-1.5 text-[11px] text-amber-700 dark:text-amber-400">Preview</span>;
}

/** The official server `id`, from the list that ships with Evo. */
export function useOfficial(id: string | null | undefined): OfficialServer | undefined {
  return useConnectedAppsStore((s) => s.providers?.official.find((o) => o.id === id));
}

/** The pool's view of an app server (status, error, tools). */
export function useServer(key: string | undefined): Connector | undefined {
  return useConnectorsStore((s) => s.servers.find((c) => c.key === key));
}

const statusWord: Record<string, string> = {
  connected: "Connected",
  connecting: "Connecting…",
  "needs-auth": "Needs sign-in",
  failed: "Failed",
  off: "Off",
};

export const serverTone = (status: string | undefined) =>
  status === "connected" ? "bg-status-ok" : status === "failed" ? "bg-destructive" : "bg-amber-500";

/** Paste a token the service issues; it goes to the system keychain. */
export function TokenDialog({
  title,
  hint,
  docs,
  open: isOpen,
  onOpenChange,
  onSubmit,
}: {
  title: string;
  hint: string;
  docs: string;
  open: boolean;
  onOpenChange(open: boolean): void;
  onSubmit(token: string): Promise<void>;
}) {
  const [token, setToken] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!isOpen) {
      setToken("");
      setError(null);
    }
  }, [isOpen]);
  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{hint} It is kept in the system keychain and sent only to that service.</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-2"
          onSubmit={async (event) => {
            event.preventDefault();
            setBusy(true);
            setError(null);
            try {
              await onSubmit(token.trim());
              onOpenChange(false);
            } catch (e) {
              setError(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          <Input type="password" autoComplete="off" spellCheck={false} value={token} onChange={(e) => setToken(e.target.value)} aria-label="Token" autoFocus className="font-mono text-xs" />
          <button type="button" onClick={() => open(docs)} className="flex w-fit items-center gap-1 text-xs text-brand hover:underline">
            How to get one <ExternalLinkIcon className="size-3" />
          </button>
          {error && (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          )}
          <DialogFooter>
            <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !token.trim()}>
              {busy ? "Connecting…" : "Connect"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** Connect official server `o`: its OAuth in the browser, or a token. */
export function useConnectOfficial() {
  const [tokenFor, setTokenFor] = React.useState<OfficialServer | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const connect = async (o: OfficialServer) => {
    if (o.auth === "token") return setTokenFor(o);
    setBusy(true);
    setError(null);
    try {
      await appsApi.connectOfficial(o.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      void useConnectedAppsStore.getState().load();
      void useConnectorsStore.getState().load();
    }
  };
  const dialog = (
    <TokenDialog
      title={`Connect ${tokenFor?.name ?? ""}`}
      hint={tokenFor?.tokenHint ?? ""}
      docs={tokenFor?.docs ?? ""}
      open={tokenFor !== null}
      onOpenChange={(isOpen) => !isOpen && setTokenFor(null)}
      onSubmit={async (token) => {
        await appsApi.connectOfficial(tokenFor!.id, token);
        void useConnectedAppsStore.getState().load();
        void useConnectorsStore.getState().load();
      }}
    />
  );
  return { connect, dialog, error, busy };
}

/**
 * A service's ways to connect (FR-20): **Directly** through its official
 * server first — data goes only to the service, no cost from Evo — then
 * **Through Composio** (accounts with labels).
 */
export function WaysDialog({ toolkit, onOpenChange }: { toolkit: Toolkit | null; onOpenChange(open: boolean): void }) {
  const key = useConnectedAppsStore((s) => s.key);
  const official = useOfficial(toolkit?.official);
  const [composio, setComposio] = React.useState<Toolkit | null>(null);
  const { connect, dialog, error, busy } = useConnectOfficial();
  const ways = toolkit ? waysFor(toolkit, key === "saved") : [];
  return (
    <>
      <Dialog open={!!toolkit && !composio} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-md">
          {toolkit && (
            <>
              <DialogHeader>
                <div className="flex items-center gap-3">
                  <ServiceTile slug={toolkit.slug} name={toolkit.name} className="size-10 rounded-xl" />
                  <div className="flex min-w-0 flex-col gap-0.5">
                    <DialogTitle>Connect {toolkit.name}</DialogTitle>
                    <DialogDescription>Choose how Evo reaches {toolkit.name}.</DialogDescription>
                  </div>
                </div>
              </DialogHeader>
              <div className="flex flex-col gap-2">
                {ways.map((way) =>
                  way.kind === "official" && official ? (
                    <button
                      key="official"
                      type="button"
                      disabled={busy}
                      onClick={() => void connect(official).then(() => official.auth === "oauth" && onOpenChange(false))}
                      className="flex flex-col gap-1 rounded-xl bg-brand/6 px-3.5 py-3 text-left shadow-[0_0_0_1px_var(--brand)] transition-colors hover:bg-brand/10"
                    >
                      <span className="flex items-center gap-2 text-[13px] font-medium">
                        Directly, with {toolkit.name}'s own server <OfficialBadge />
                        {official.preview && <PreviewBadge />}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        Data goes only to {toolkit.name}; Evo adds no cost.{" "}
                        {official.auth === "token" ? "Sign in with a token from " + toolkit.name + "." : `Sign in with ${toolkit.name} in your browser.`}
                      </span>
                    </button>
                  ) : way.kind === "composio" ? (
                    <button
                      key="composio"
                      type="button"
                      onClick={() => setComposio(toolkit)}
                      className="flex flex-col gap-1 rounded-xl px-3.5 py-3 text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/50"
                    >
                      <span className="text-[13px] font-medium">Through Composio</span>
                      <span className="text-xs text-muted-foreground">Several accounts with labels; calls go through Composio with your key.</span>
                    </button>
                  ) : null,
                )}
                {!ways.some((w) => w.kind === "composio") && toolkit.composio && (
                  <p className="text-xs text-muted-foreground">Add a Composio key in Connected apps settings to connect it through Composio too.</p>
                )}
                {busy && <p className="text-xs text-muted-foreground">Finish signing in to {toolkit.name} in your browser…</p>}
                {error && (
                  <p role="alert" className="text-xs text-destructive">
                    {error}
                  </p>
                )}
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
      <ConnectDialog
        toolkit={composio}
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            setComposio(null);
            onOpenChange(false);
          }
        }}
      />
      {dialog}
    </>
  );
}

/**
 * Zapier as a provider, in the Connected apps settings panel (FR-24):
 * Connect (its sign-in) or a connection token; once connected its status,
 * Choose actions on Zapier, Open its page and Disconnect; the tasks note.
 */
export function ZapierCard({ onOpen }: { onOpen?(): void }) {
  const zapier = useConnectedAppsStore((s) => s.providers?.zapier);
  const server = useServer(zapier?.key);
  const [tokenOpen, setTokenOpen] = React.useState(false);
  const [confirm, setConfirm] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  if (!zapier) return null;
  const connect = async () => {
    setBusy(true);
    setError(null);
    try {
      await appsApi.connectZapier();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      void useConnectedAppsStore.getState().load();
      void useConnectorsStore.getState().load();
    }
  };
  return (
    <section className="flex flex-col gap-3 rounded-xl bg-muted/40 p-4 dark:bg-accent/40">
      <div className="flex items-center gap-3">
        <ServiceTile slug="zapier" name="Zapier" className="size-9 rounded-[10px]" />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="flex items-center gap-1.5 text-[13px] font-medium">
            <ZapIcon className="size-3.5 text-amber-500" /> Zapier
          </span>
          {zapier.connected ? (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className={cn("size-1.5 rounded-full", serverTone(server?.status))} />
              {statusWord[server?.status ?? "connecting"]}
              {server?.status === "connected" && ` · ${server.tools.length} actions`}
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">Thousands of apps through your Zapier account.</span>
          )}
        </div>
        {zapier.connected ? (
          <div className="flex items-center gap-1">
            {onOpen && (
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
                  void useConnectedAppsStore.getState().load();
                  void useConnectorsStore.getState().load();
                }}
              >
                Disconnect
              </Button>
            ) : (
              <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(true)}>
                Disconnect
              </Button>
            )}
          </div>
        ) : (
          <Button size="sm" disabled={busy} onClick={() => void connect()}>
            {busy ? "Signing in…" : "Connect"}
          </Button>
        )}
      </div>
      <p className="text-xs text-muted-foreground">
        Its tools are the actions you enable for your server on Zapier.{" "}
        <button type="button" onClick={() => open(zapier.actions)} className="text-brand hover:underline">
          Choose actions
        </button>
        . Each successful call uses {zapier.tasksPerCall} tasks of your Zapier plan, at Zapier's rate; failed calls use none.
      </p>
      {!zapier.connected && (
        <button type="button" onClick={() => setTokenOpen(true)} className="flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
          <KeyRoundIcon className="size-3" /> Use a connection token instead
        </button>
      )}
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      {zapier.connected && server && server.status !== "connected" && server.status !== "connecting" && (
        <Trouble server={server} docs={zapier.docs} name="Zapier" onRetry={() => void connect()} />
      )}
      <TokenDialog
        title="Connect Zapier with a token"
        hint="A connection token from mcp.zapier.com → your server → Connect."
        docs={zapier.docs}
        open={tokenOpen}
        onOpenChange={setTokenOpen}
        onSubmit={async (token) => {
          await appsApi.connectZapier(token);
          void useConnectedAppsStore.getState().load();
          void useConnectorsStore.getState().load();
        }}
      />
    </section>
  );
}

/**
 * What is wrong with an app server, in the person's words: a sign-in that is
 * missing or refused, an address that does not answer, or the server's own
 * message cut short. The raw text stays available under Details.
 */
export function problemOf(server: Pick<Connector, "status" | "error"> | undefined, name: string): string | null {
  if (!server || server.status === "connected" || server.status === "connecting" || server.status === "off") return null;
  const raw = (server.error ?? "").toLowerCase();
  if (server.status === "needs-auth" || /auth required|unauthori[sz]ed|\b401\b|\b403\b|invalid_token/.test(raw))
    return `${name} needs you to sign in.`;
  if (/timed out|no answer|connection refused|dns|could not connect|error sending request|connect error/.test(raw)) return `${name} does not answer right now.`;
  const first = (server.error ?? "").split(/[\n.]/)[0]?.trim();
  return first && first.length < 140 && !first.includes("::") ? first : `${name} could not be reached.`;
}

/** Failed or needing sign-in, with the reason and the service's docs (FR-26). */
export function Trouble({ server, docs, name, onRetry }: { server: Connector | undefined; docs: string; name: string; onRetry(): void }) {
  const problem = problemOf(server, name);
  if (!server || !problem) return null;
  return (
    <div className="flex flex-col gap-2 rounded-xl bg-amber-500/8 px-3.5 py-3 text-[13px] shadow-[0_0_0_1px_rgb(245_158_11/0.3)]">
      <p className="font-medium">{problem}</p>
      {server.error && (
        <details className="text-xs text-muted-foreground">
          <summary className="cursor-default select-none hover:text-foreground">Details</summary>
          <p className="mt-1 font-mono break-words">{server.error}</p>
        </details>
      )}
      <div className="flex gap-2">
        <Button size="xs" onClick={onRetry}>
          {problem.endsWith("sign in.") ? "Sign in" : "Try again"}
        </Button>
        <Button size="xs" variant="ghost" onClick={() => open(docs)}>
          {name}'s documentation <ExternalLinkIcon />
        </Button>
      </div>
    </div>
  );
}

/** A connected official server: what it is, its state, sign in again, Disconnect. */
export function OfficialHeader({ id }: { id: string }) {
  const o = useOfficial(id);
  const server = useServer(o?.key);
  const { connect, dialog, error, busy } = useConnectOfficial();
  const [confirm, setConfirm] = React.useState(false);
  if (!o) return null;
  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-start gap-3">
        <ServiceTile slug={o.composio[0] ?? o.id} name={o.name} className="size-11 rounded-xl" />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <h2 className="flex items-center gap-2 truncate text-base font-semibold">
            {o.name} <OfficialBadge />
            {o.preview && <PreviewBadge />}
          </h2>
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className={cn("size-1.5 rounded-full", serverTone(server?.status))} />
            {statusWord[server?.status ?? "connecting"]}
            {server?.status === "connected" && ` · ${server.tools.length} tools`}
          </p>
        </div>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void connect(o)}>
          Sign in again
        </Button>
        {confirm ? (
          <Button
            size="sm"
            variant="destructive"
            onBlur={() => setConfirm(false)}
            onClick={async () => {
              await appsApi.disconnectOfficial(o.id).catch(() => undefined);
              void useConnectedAppsStore.getState().load();
              void useConnectorsStore.getState().load();
            }}
          >
            Disconnect
          </Button>
        ) : (
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(true)}>
            Disconnect
          </Button>
        )}
      </header>
      <p className="text-xs text-muted-foreground">
        {o.name}'s own MCP server: data goes only to {o.name} ({new URL(o.url).host}), and Evo adds no cost. Its tools that are not
        marked read-only ask first.{" "}
        <button type="button" onClick={() => open(o.docs)} className="text-brand hover:underline">
          Documentation
        </button>
      </p>
      <Trouble server={server} docs={o.docs} name={o.name} onRetry={() => void connect(o)} />
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      {dialog}
    </div>
  );
}

/** Connected Zapier: choose actions on Zapier, its tasks, Disconnect. */
export function ZapierHeader() {
  const zapier = useConnectedAppsStore((s) => s.providers?.zapier);
  const server = useServer(zapier?.key);
  const [confirm, setConfirm] = React.useState(false);
  if (!zapier) return null;
  const retry = () =>
    void appsApi
      .connectZapier()
      .catch(() => undefined)
      .finally(() => void useConnectorsStore.getState().load());
  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-start gap-3">
        <ServiceTile slug="zapier" name="Zapier" className="size-11 rounded-xl" />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <h2 className="truncate text-base font-semibold">Zapier</h2>
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className={cn("size-1.5 rounded-full", serverTone(server?.status))} />
            {statusWord[server?.status ?? "connecting"]}
            {server?.status === "connected" && ` · ${server.tools.length} actions`}
          </p>
        </div>
        <Button size="sm" variant="outline" onClick={() => open(zapier.actions)}>
          Choose actions <ExternalLinkIcon />
        </Button>
        {confirm ? (
          <Button
            size="sm"
            variant="destructive"
            onBlur={() => setConfirm(false)}
            onClick={async () => {
              await appsApi.disconnectZapier().catch(() => undefined);
              void useConnectedAppsStore.getState().load();
              void useConnectorsStore.getState().load();
            }}
          >
            Disconnect
          </Button>
        ) : (
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => setConfirm(true)}>
            Disconnect
          </Button>
        )}
      </header>
      <p className="text-xs text-muted-foreground">
        Its tools are the actions you enabled for this server on Zapier. Each successful call uses {zapier.tasksPerCall} tasks of your
        Zapier plan, at Zapier's rate; failed calls use none.
      </p>
      <Trouble server={server} docs={zapier.docs} name="Zapier" onRetry={retry} />
    </div>
  );
}
