import * as React from "react";
import { ExternalLinkIcon, KeyRoundIcon } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useConnectedAppsStore } from "@/stores/connected-apps-store";

// The Composio project key (docs/features/connected-apps/spec.md, FR-1, FR-2):
// what Composio is and what goes through it, the field, Save (checked with
// Composio first) and Remove key. The key itself never comes back to the page.

const KEYS_PAGE = "https://platform.composio.dev/";

/** What Composio is and what passes through it (FR-2). */
export function ComposioNote() {
  return (
    <p className="text-xs text-muted-foreground">
      Connected apps run on Composio with your own project key. Composio keeps the services' sign-ins, and every
      connected-app tool call and its result go through Composio. Evo never sees a service's password or token.
    </p>
  );
}

export function KeyCard({ compact }: { compact?: boolean }) {
  const key = useConnectedAppsStore((s) => s.key);
  const status = useConnectedAppsStore((s) => s.status);
  const reason = useConnectedAppsStore((s) => s.reason);
  const [value, setValue] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [editing, setEditing] = React.useState(false);

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

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await useConnectedAppsStore.getState().removeKey();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saved = key === "saved" && !editing;
  return (
    <section className="flex flex-col gap-3 rounded-xl bg-muted/40 p-4 dark:bg-accent/40">
      <div className="flex items-center gap-2">
        <KeyRoundIcon className="size-4 text-muted-foreground" />
        <h2 className="text-[13px] font-medium">Composio project key</h2>
        {key === "saved" && (
          <span className="ml-auto text-xs text-muted-foreground">
            {status === "unavailable" ? "Saved · Composio unavailable" : "Saved in the system keychain"}
          </span>
        )}
      </div>
      {!compact && <ComposioNote />}
      {saved ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs text-muted-foreground">ak_••••••••••••</span>
          <Button size="xs" variant="outline" disabled={busy} onClick={() => setEditing(true)}>
            Replace
          </Button>
          <Button size="xs" variant="ghost" disabled={busy} onClick={() => void remove()}>
            Remove key
          </Button>
          {status === "unavailable" && (
            <Button size="xs" variant="ghost" disabled={busy} onClick={() => void useConnectedAppsStore.getState().retry().catch(() => undefined)}>
              Retry
            </Button>
          )}
        </div>
      ) : (
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (value.trim()) void save();
          }}
        >
          <Input
            type="password"
            autoComplete="off"
            spellCheck={false}
            placeholder="ak_…"
            aria-label="Composio project key"
            value={value}
            onChange={(event) => setValue(event.target.value)}
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
        </form>
      )}
      {key !== "saved" && (
        <button
          type="button"
          onClick={() => void openUrl(KEYS_PAGE).catch(() => undefined)}
          className="flex w-fit items-center gap-1 text-xs text-brand hover:underline"
        >
          Get a project key: Composio → your project → Settings → API keys
          <ExternalLinkIcon className="size-3" />
        </button>
      )}
      {(error || (key === "invalid" && reason) || (status === "unavailable" && reason)) && (
        <p role="alert" className="text-xs text-destructive">
          {error ?? reason}
        </p>
      )}
    </section>
  );
}
