// Turning a channel's board on asks for its key (docs/features/evo-mates-
// threads-board/spec.md, FR-8, D6): 2 – 6 capitals, proposed from the name.

import * as React from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

export function validKey(key: string): boolean {
  return /^[A-Z][A-Z0-9]{1,5}$/.test(key);
}

export function BoardKeyDialog({
  open,
  onOpenChange,
  channelName,
  proposed,
  onTurnOn,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  channelName: string;
  proposed: string;
  onTurnOn(key: string): Promise<void>;
}) {
  const [key, setKey] = React.useState(proposed);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (open) {
      setKey(proposed);
      setError(null);
    }
  }, [open, proposed]);
  const submit = () => {
    if (!validKey(key)) return setError("2 to 6 capital letters or digits, a letter first.");
    void onTurnOn(key)
      .then(() => onOpenChange(false))
      .catch((e) => setError(String(e).replace(/^Error:\s*/, "")));
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Turn on a board for #{channelName}</DialogTitle>
          <DialogDescription>Cards are numbered with this key, like {key || "KEY"}-12. It cannot change later.</DialogDescription>
        </DialogHeader>
        <input
          autoFocus
          value={key}
          aria-label="Board key"
          onChange={(e) => {
            setKey(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 6));
            setError(null);
          }}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          className="h-9 w-full rounded-lg border border-border bg-background px-3 font-mono text-[14px] tracking-wide outline-none focus-visible:ring-2 focus-visible:ring-brand/20"
        />
        {error && <p className="text-[12.5px] text-destructive">{error}</p>}
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={!key}>
            Turn on board
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
