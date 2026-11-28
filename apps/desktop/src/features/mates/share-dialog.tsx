// Share lines into another conversation (docs/features/evo-mates-threads-
// board/spec.md, FR-16): one line, several, or a stretch, into a mate's chat
// or a channel, with a note. The receiver shows a "Shared from …" card with a
// link back; its mate reads the lines as shared, not as the person's request.
// Both conversations then list each other under Linked (FR-19).

import * as React from "react";
import { HashIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { notify } from "@/features/notifications/notify";
import { linksApi } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { excerpt, sharePrompt } from "./message-markup";
import { messageLink } from "./links-logic";

export interface Sharing {
  /** The conversation the lines come from. */
  chat: string;
  /** How the receiver names it: "#Launch room" or a mate's name. */
  place: string;
  lines: { id: string; author: string; text: string }[];
}

async function send(target: string, item: Sharing, note: string) {
  const store = useMatesStore.getState();
  const [kind, id] = [target.slice(0, target.indexOf(":")), target.slice(target.indexOf(":") + 1)];
  const to = kind === "mate" ? store.mates.find((m) => m.id === id)?.chatId : store.channels.find((c) => c.id === id)?.chatId;
  if (!to) throw new Error("that conversation is gone");
  const prompt = sharePrompt(item.place, messageLink(item.chat, item.lines[0]?.id ?? ""), item.lines, note);
  await linksApi
    .record({ fromChat: item.chat, fromLines: item.lines.map((l) => l.id), excerpt: excerpt(item.lines.map((l) => l.text).join(" "), 200), toChat: to, note: note.trim() })
    .catch(() => undefined);
  if (kind === "mate") await store.open(id);
  else await store.openChannel(id);
  // The place opens first: the engine store sends to the chat it shows.
  await useEngineStore.getState().whenSetUp(useEngineStore.getState().activeId ?? "");
  await useEngineStore.getState().sendPrompt(prompt);
}

export function ShareDialog({ item, onClose }: { item: Sharing | null; onClose(): void }) {
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
    ...channels.filter((c) => !c.hidden && c.chatId !== item?.chat).map((c) => ({ key: `channel:${c.id}`, name: `#${c.name}`, detail: `${c.members.length} mates`, mate: null })),
    ...mates.filter((m) => !m.hidden && m.chatId !== item?.chat).map((m) => ({ key: `mate:${m.id}`, name: m.name, detail: m.role, mate: m })),
  ];
  const count = item?.lines.length ?? 0;
  return (
    <Dialog open={item !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            Share {count} {count === 1 ? "message" : "messages"}
          </DialogTitle>
          <DialogDescription>From {item?.place}. The receiver sees where they came from, with a link back.</DialogDescription>
        </DialogHeader>
        <div className="max-h-28 overflow-y-auto rounded-xl bg-muted/50 px-3 py-2 text-[12.5px] text-muted-foreground">
          {item?.lines.map((l) => (
            <p key={l.id} className="line-clamp-2">
              <span className="font-medium text-foreground">{l.author}</span> {l.text}
            </p>
          ))}
        </div>
        <ul className="flex max-h-56 flex-col gap-0.5 overflow-y-auto" role="listbox" aria-label="Share to">
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
          placeholder="Add a note: what should they do with it? (optional)"
          rows={2}
          aria-label="Note"
          className="w-full resize-none rounded-xl bg-muted/60 px-3 py-2 text-[13px] outline-none placeholder:text-muted-foreground"
        />
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!target || busy || !item || count === 0}
            onClick={() => {
              if (!target || !item) return;
              setBusy(true);
              void send(target, item, note)
                .catch((error) => notify({ source: "mates", level: "warning", title: "Couldn't share", body: String(error), system: false }))
                .finally(() => {
                  setBusy(false);
                  onClose();
                });
            }}
          >
            Share
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
