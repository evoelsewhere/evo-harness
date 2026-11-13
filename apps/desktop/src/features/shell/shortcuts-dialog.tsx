import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { useUiStore } from "@/stores/ui-store";
import { shortcutGroups } from "./shortcuts";

/** Every keyboard shortcut, grouped by area (docs/features/ux-polish/spec.md, FR-7). */
export function ShortcutsDialog() {
  const open = useUiStore((s) => s.shortcutsOpen);
  const setOpen = useUiStore((s) => s.setShortcutsOpen);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-h-[80vh] gap-4 overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>What each key does across evo.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-x-8 gap-y-5 sm:grid-cols-2">
          {shortcutGroups().map((group) => (
            <section key={group.title} className="flex flex-col gap-1.5">
              <h3 className="text-xs font-medium text-muted-foreground">{group.title}</h3>
              <ul className="flex flex-col">
                {group.items.map((item) => (
                  <li key={item.label} className="flex items-center justify-between gap-3 py-1 text-[13px]">
                    <span className="min-w-0 truncate" title={item.label}>
                      {item.label}
                    </span>
                    <KbdGroup className="shrink-0">
                      {item.keys.map((key) => (
                        <Kbd key={key}>{key}</Kbd>
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
