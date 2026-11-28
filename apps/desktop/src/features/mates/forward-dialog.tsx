// Forward a message (docs/features/evo-mates-messaging/spec.md, FR-10): pick
// a mate or a channel, add a note, and it goes there marked "Forwarded from
// <name>" — the receiver reads it as quoted, not as the person's instruction.

import * as React from "react";
import { HashIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { notify } from "@/features/notifications/notify";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { forwardPrompt } from "./message-markup";

export interface Forwarding {
  /** Who wrote it: a mate's name, or "You". */
  from: string;
  text: string;
  /** The place it comes from, left out of the list. */
  chat: string;
}

/** Forward `text` to the place `target` (`mate:<id>` or `channel:<id>`). */
async function send(target: string, item: Forwarding, note: string) {
  const store = useMatesStore.getState();
  const [kind, id] = [target.slice(0, target.indexOf(":")), target.slice(target.indexOf(":") + 1)];
  if (kind === "mate") await store.open(id);
  else await store.openChannel(id);
  // The place opens first: the engine store sends to the chat it shows.
  await useEngineStore.getState().whenSetUp(useEngineStore.getState().activeId ?? "");
  await useEngineStore.getState().sendPrompt(forwardPrompt(item.from, item.text, note));
}

export function ForwardDialog({ item, onClose }: { item: Forwarding | null; onClose(): void }) {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const [target, setTarget] = React.useState<string | null>(null);
  const [note, setNote] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  React.useEffect(() => {
    setTarget(null);
    setNote("");
  }, [item]);
  const places = [
    ...mates.filter((m) => !m.hidden && m.chatId !== item?.chat).map((m) => ({ key: `mate:${m.id}`, name: m.name, detail: m.role, mate: m })),
    ...channels.filter((c) => !c.hidden && c.chatId !== item?.chat).map((c) => ({ key: `channel:${c.id}`, name: `#${c.name}`, detail: `${c.members.length} mates`, mate: null })),
  ];
  return (
    <Dialog open={item !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Forward</DialogTitle>
          <DialogDescription className="line-clamp-2">
            {item?.from}: {item?.text}
          </DialogDescription>
        </DialogHeader>
        <ul className="flex max-h-64 flex-col gap-0.5 overflow-y-auto" role="listbox" aria-label="Forward to">
          {places.map((place) => (
            <li key={place.key}>
              <button
                type="button"
                role="option"
                aria-selected={target === place.key}
                onClick={() => setTarget(place.key)}
                className={cn("flex w-full items-center gap-2.5 rounded-xl px-2 py-1.5 text-left hover:bg-accent", target === place.key && "bg-accent")}
              >
                {place.mate ? (
                  <MateFace avatar={place.mate.avatar} size={28} plate={false} label={place.name} />
                ) : (
                  <span className="flex size-7 items-center justify-center rounded-[9px] bg-muted text-muted-foreground">
                    <HashIcon className="size-3.5" />
                  </span>
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium text-foreground">{place.name}</span>
                  <span className="block truncate text-xs text-muted-foreground">{place.detail}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
        <textarea
          value={note}
          onChange={(event) => setNote(event.target.value)}
          placeholder="Add a note (optional)"
          rows={2}
          aria-label="Note"
          className="w-full resize-none rounded-xl bg-muted/60 px-3 py-2 text-[13px] outline-none placeholder:text-muted-foreground"
        />
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!target || busy || !item}
            onClick={() => {
              if (!target || !item) return;
              setBusy(true);
              void send(target, item, note)
                .catch((error) => notify({ source: "mates", level: "warning", title: "Couldn't forward", body: String(error), system: false }))
                .finally(() => {
                  setBusy(false);
                  onClose();
                });
            }}
          >
            Forward
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
