// One question before an action that loses or rewrites work: discarding,
// committing while the agent works, amending a pushed commit, reverting a
// turn (docs/features/source-control/spec.md, FR-5, FR-11, FR-13).

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

export interface Confirm {
  title: string;
  body: string;
  /** The button that goes ahead ("Discard", "Commit"). */
  action: string;
  destructive?: boolean;
  /** A second way to go ahead ("Revert anyway"). */
  alternative?: { label: string; run(): void };
  run(): void;
}

export function ConfirmDialog({ confirm, onClose }: { confirm: Confirm | null; onClose(): void }) {
  return (
    <Dialog open={confirm !== null} onOpenChange={(open) => !open && onClose()}>
      {confirm && (
        <DialogContent showCloseButton={false} className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{confirm.title}</DialogTitle>
            {confirm.body && <DialogDescription className="whitespace-pre-line">{confirm.body}</DialogDescription>}
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            {confirm.alternative && (
              <Button
                variant="outline"
                onClick={() => {
                  onClose();
                  confirm.alternative?.run();
                }}
              >
                {confirm.alternative.label}
              </Button>
            )}
            <Button
              variant={confirm.destructive ? "destructive" : "default"}
              autoFocus
              onClick={() => {
                onClose();
                confirm.run();
              }}
            >
              {confirm.action}
            </Button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
