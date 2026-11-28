// Every Evo Mates shortcut on one sheet (docs/features/evo-mates-ui-v2/spec.md, FR-8).

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { SHORTCUTS } from "./use-mates-keys";

export function ShortcutsSheet() {
  const open = useMatesUiStore((s) => s.shortcutsOpen);
  const setOpen = useMatesUiStore((s) => s.setShortcutsOpen);
  const groups = ["Move", "Conversation", "Window"] as const;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Move around Evo Mates without the mouse.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          {groups.map((group) => (
            <section key={group} className="flex flex-col gap-1">
              <h3 className="text-xs font-medium text-muted-foreground">{group}</h3>
              <ul className="flex flex-col">
                {SHORTCUTS.filter((s) => s.group === group).map((s) => (
                  <li key={s.label} className="flex items-center justify-between gap-4 py-1 text-[13px] text-foreground">
                    {s.label}
                    <KbdGroup>
                      {s.keys.map((k) => (
                        <Kbd key={k}>{k}</Kbd>
                      ))}
                    </KbdGroup>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
