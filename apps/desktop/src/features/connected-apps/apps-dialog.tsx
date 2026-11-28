import { create } from "zustand";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ConnectedAppsPage } from "./connected-apps-page";

// Evo Mates has no Settings window of its own: its Connected apps page opens
// as a dialog from the sidebar or a mate's Apps tab (connected-apps FR-3).

export const useConnectedAppsDialog = create<{ open: boolean; setOpen(open: boolean): void }>()((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
}));

export const openConnectedApps = () => useConnectedAppsDialog.getState().setOpen(true);

export function ConnectedAppsDialog() {
  const open = useConnectedAppsDialog((s) => s.open);
  const setOpen = useConnectedAppsDialog((s) => s.setOpen);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="flex max-h-[85vh] max-w-2xl flex-col rounded-[22px]">
        <DialogHeader>
          <DialogTitle>Connected apps</DialogTitle>
          <DialogDescription>Services your mates can use, each account with its own label.</DialogDescription>
        </DialogHeader>
        <div className="scrollbar-slim -mx-1 min-h-0 flex-1 overflow-y-auto px-1">
          <ConnectedAppsPage />
        </div>
      </DialogContent>
    </Dialog>
  );
}
