import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { inTauri } from "@/lib/platform";

// Quitting while chats work asks once (docs/features/shell-hosted-engines/spec.md,
// FR-11): from the tray's Quit, or closing the last window with the tray off.
// The shell picks one window to ask (`app://quit-requested`); a main window it
// built to ask reads the question when it loads (`quit_pending`). "Quit
// anyway" stops every chat's engine and exits (`quit_confirmed`).

/** "1 chat is working. Quit anyway?" / "3 chats are working. Quit anyway?" */
export function quitTitle(working: number): string {
  return working === 1 ? "1 chat is working. Quit anyway?" : `${working} chats are working. Quit anyway?`;
}

export function QuitDialog() {
  const [working, setWorking] = React.useState<number | null>(null);

  React.useEffect(() => {
    if (!inTauri) return;
    const label = getCurrentWindow().label;
    let gone = false;
    let stop: (() => void) | undefined;
    void listen<{ working: number; window?: string }>("app://quit-requested", ({ payload }) => {
      if (payload.window && payload.window !== label) return;
      setWorking(payload.working);
    })
      .then((unlisten) => {
        if (gone) unlisten();
        else stop = unlisten;
      })
      .catch(() => undefined);
    // Asked before this window had loaded.
    void invoke<number | null>("quit_pending")
      .then((pending) => {
        if (!gone && pending != null) setWorking(pending);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  const answer = (quit: boolean) => {
    setWorking(null);
    void invoke(quit ? "quit_confirmed" : "quit_dismissed").catch(() => undefined);
  };

  return (
    <Dialog
      open={working !== null}
      onOpenChange={(open) => {
        if (!open) answer(false);
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{quitTitle(working ?? 0)}</DialogTitle>
          <DialogDescription>
            Quitting stops every chat. A run in progress ends where it is, and questions waiting for you are dropped.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => answer(false)}>
            Cancel
          </Button>
          <Button variant="destructive" autoFocus onClick={() => answer(true)}>
            Quit anyway
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
