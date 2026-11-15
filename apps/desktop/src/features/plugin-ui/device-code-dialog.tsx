// A plugin's device-code sign-in waits for the person
// (docs/features/plugin-settings/spec.md, FR-22): evo shows the code and the
// address, opens it on request, and closes when the sign-in ends — wherever
// it started (the plugin page, a chat's question, a view).

import * as React from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CopyIcon, ExternalLinkIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import { inTauri } from "@/lib/platform";
import { plugins, type DeviceCode } from "@/lib/plugins";

interface Waiting {
  id: string;
  account: string;
  plugin: string;
  service: string;
  device: DeviceCode;
}

const useWaiting = create<{ current: Waiting | null; set(w: Waiting | null): void }>((set) => ({
  current: null,
  set: (current) => set({ current }),
}));

export function DeviceCodeDialog() {
  const current = useWaiting((s) => s.current);
  const [copied, setCopied] = React.useState(false);
  React.useEffect(() => {
    if (!inTauri) return;
    const offs = [
      listen<Waiting>("plugins://device-code", ({ payload }) => {
        setCopied(false);
        useWaiting.getState().set(payload);
      }),
      // The sign-in ended (done, cancelled, failed): the code is no longer needed.
      listen<{ id: string; account: string; state: string }>("plugins://accounts", ({ payload }) => {
        const shown = useWaiting.getState().current;
        if (shown && shown.id === payload.id && shown.account === payload.account && payload.state !== "signingIn") useWaiting.getState().set(null);
      }),
    ];
    return () => offs.forEach((off) => void off.then((stop) => stop()));
  }, []);
  const cancel = () => {
    if (current) void plugins.cancelSignIn(current.id, current.account);
    useWaiting.getState().set(null);
  };
  return (
    <Dialog open={!!current} onOpenChange={(open) => !open && cancel()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Sign in to {current?.service}</DialogTitle>
          <DialogDescription>
            For {current?.plugin}. Open the address, then enter this code there. This closes when {current?.service} confirms.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col items-center gap-3 py-2">
          <div className="flex items-center gap-2">
            <code className="rounded-lg bg-muted px-4 py-2 font-mono text-2xl font-semibold tracking-widest select-all">{current?.device.code}</code>
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label="Copy the code"
              onClick={() => {
                if (!current) return;
                void navigator.clipboard.writeText(current.device.code).then(() => setCopied(true));
              }}
            >
              <CopyIcon />
            </Button>
          </div>
          <span className="text-xs text-muted-foreground">{copied ? "Copied" : current?.device.url}</span>
          <span className="flex items-center gap-2 text-xs text-muted-foreground">
            <Spinner className="size-3" /> Waiting for you to confirm…
          </span>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={cancel}>
            Cancel
          </Button>
          <Button onClick={() => current && void openUrl(current.device.complete ?? current.device.url)}>
            <ExternalLinkIcon /> Open {current ? new URL(current.device.url).host : ""}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
