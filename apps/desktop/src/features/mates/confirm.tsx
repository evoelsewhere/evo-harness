import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

/** Confirm a destructive mate action; focus starts on Cancel (ux.md 5). */
export function ConfirmDelete({
  open,
  onOpenChange,
  title,
  description,
  onConfirm,
  action = "Delete",
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  title: string;
  description: string;
  onConfirm(): void;
  /** The confirm button's word. */
  action?: string;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" autoFocus onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={() => {
              onOpenChange(false);
              onConfirm();
            }}
          >
            {action}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
