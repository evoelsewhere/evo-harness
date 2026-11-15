// A plugin asks for an optional permission while in use
// (docs/features/plugin-settings/spec.md, FR-9): evo asks the person once;
// Allow grants it, Not now is remembered so the plugin is not asked again.

import * as React from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ShieldCheckIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { inTauri } from "@/lib/platform";
import { plugins } from "@/lib/plugins";

interface Request {
  plugin: string;
  name: string;
  permission: string;
  label: string;
  reason?: string | null;
}

const useRequests = create<{ queue: Request[]; push(r: Request): void; shift(): void }>((set) => ({
  queue: [],
  push: (r) => set((s) => (s.queue.some((q) => q.plugin === r.plugin && q.permission === r.permission) ? s : { queue: [...s.queue, r] })),
  shift: () => set((s) => ({ queue: s.queue.slice(1) })),
}));

export function PermissionRequestDialog() {
  const request = useRequests((s) => s.queue[0]);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!inTauri) return;
    const off = listen<Request>("plugin-ui://permission-request", ({ payload }) => useRequests.getState().push(payload));
    return () => void off.then((stop) => stop());
  }, []);
  async function answer(allow: boolean) {
    if (!request) return;
    setBusy(true);
    setError(null);
    try {
      await plugins.answerOptional(request.plugin, request.permission, allow);
      useRequests.getState().shift();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog open={!!request} onOpenChange={(open) => !open && !busy && void answer(false)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Allow {request?.name} to {request?.label.toLowerCase()}?</DialogTitle>
          <DialogDescription>{request?.reason ?? "It works without this; it asks because you are using it now."}</DialogDescription>
        </DialogHeader>
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <ShieldCheckIcon className="size-3.5" />
          Your answer is kept. You can change it in Customize › Plugins › Permissions.
        </p>
        {error && <p className="text-xs text-destructive">{error}</p>}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={() => void answer(false)}>
            Not now
          </Button>
          <Button disabled={busy} onClick={() => void answer(true)}>
            Allow
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
