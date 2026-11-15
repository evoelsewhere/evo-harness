import * as React from "react";
import { create } from "zustand";
import { ShieldCheckIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { plugins, type ConsentState, type PermissionRow } from "@/lib/plugins";
import { PermissionNotes } from "./plugin-page/parts";

// Install-time consent (docs/features/plugin-ui/spec.md, FR-3;
// docs/features/plugin-settings/spec.md, FR-8, FR-15, FR-17): after an
// install, an update, or turning a plugin on, the permissions it asks for —
// its views' and its code's — are listed in plain words with the plugin's
// reasons and what evo cannot enforce, and approved once.

interface Ask {
  id: string;
  name: string;
  permissions: PermissionRow[];
  /** An update waiting for these permissions. */
  pendingVersion?: string | null;
}

const useConsent = create<{ ask: Ask | null; set(ask: Ask | null): void }>((set) => ({
  ask: null,
  set: (ask) => set({ ask }),
}));

/** Ask the person to approve what `state` still needs, if anything. */
export function askConsent(state: Pick<ConsentState, "id" | "needsConsent" | "pendingVersion">, name: string): void {
  if (state.needsConsent.length === 0) return;
  useConsent.getState().set({ id: state.id, name, permissions: state.needsConsent, pendingVersion: state.pendingVersion });
}

/** The one consent dialog of Customize › Plugins. */
export function PluginConsentDialog() {
  const ask = useConsent((s) => s.ask);
  const close = () => useConsent.getState().set(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => setError(null), [ask]);

  async function approve() {
    if (!ask) return;
    setBusy(true);
    setError(null);
    try {
      await plugins.approve(ask.id, ask.permissions.map((p) => p.id));
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={ask !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {ask?.pendingVersion ? `Update ${ask.name} to ${ask.pendingVersion}?` : `Allow ${ask?.name ?? "this plugin"}?`}
          </DialogTitle>
          <DialogDescription>
            {ask?.pendingVersion
              ? "The new version asks for more. Until you allow it, the version you have keeps running."
              : "It asks to:"}
          </DialogDescription>
        </DialogHeader>
        <ul className="flex flex-col gap-1.5">
          {ask?.permissions.map((permission) => (
            <li key={permission.id} className="flex items-start gap-2 text-sm">
              <ShieldCheckIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span>
                  {permission.label}
                  {permission.id.startsWith("network:") ? null : (
                    <span className="ml-1.5 font-mono text-[11px] text-muted-foreground">{permission.id}</span>
                  )}
                </span>
                <PermissionNotes permission={permission} />
              </span>
            </li>
          ))}
        </ul>
        <p className="text-xs text-muted-foreground">You can take any of these back later in Customize › Plugins.</p>
        {error && <p className="text-xs text-destructive">{error}</p>}
        <DialogFooter>
          <Button variant="ghost" onClick={close} disabled={busy}>
            Not now
          </Button>
          <Button onClick={() => void approve()} disabled={busy}>
            {busy ? "Allowing…" : ask?.pendingVersion ? "Allow and update" : "Allow and turn on"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
