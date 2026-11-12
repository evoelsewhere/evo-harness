// Edit sidebar: one checkbox per sidebar item; a change shows at once
// (docs/features/sidebar-items/spec.md, FR-4).

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useUiStore } from "@/stores/ui-store";
import { SIDEBAR_ITEMS, isShown } from "./sidebar-items";

export function EditSidebarDialog({ open, onOpenChange }: { open: boolean; onOpenChange(open: boolean): void }) {
  const saved = useUiStore((s) => s.sidebarShown);
  const setShown = useUiStore((s) => s.setSidebarShown);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Edit sidebar</DialogTitle>
          <DialogDescription>Choose which items appear in your sidebar. The others are under More.</DialogDescription>
        </DialogHeader>
        <div className="-mx-2 flex flex-col">
          {SIDEBAR_ITEMS.map((item) => {
            const Icon = item.icon;
            return (
              <label
                key={item.id}
                className="flex h-9 cursor-default items-center gap-3 rounded-lg px-2 text-sm transition-colors hover:bg-accent/60"
              >
                <Checkbox checked={isShown(item, saved)} onCheckedChange={(checked) => setShown(item.id, checked)} />
                <Icon className="size-4 text-muted-foreground" />
                <span>{item.label}</span>
              </label>
            );
          })}
        </div>
        <DialogFooter>
          <DialogClose render={<Button size="sm" />}>Done</DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
