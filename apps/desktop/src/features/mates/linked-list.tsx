// A conversation's linked conversations (docs/features/evo-mates-threads-
// board/spec.md, FR-19): what was shared into it and out of it, newest
// first, each opening the other place. Shown in a channel's Linked dialog
// and on a mate's Overview.

import * as React from "react";
import { ArrowDownLeftIcon, ArrowUpRightIcon, HashIcon } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { MateFace } from "@/components/mate-face/mate-face";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { formatRelative } from "@/lib/format";
import { linksApi, type Share } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { useMatesStore } from "@/stores/mates-store";

export function useLinked(chat: string): Share[] {
  const [list, setList] = React.useState<Share[]>([]);
  React.useEffect(() => {
    let gone = false;
    const read = () =>
      void linksApi
        .linked(chat)
        .then((all) => !gone && setList(all))
        .catch(() => undefined);
    read();
    const stop = inTauri ? listen("mates://links", read) : null;
    return () => {
      gone = true;
      void stop?.then((off) => off());
    };
  }, [chat]);
  return list;
}

export function LinkedList({ chat, onOpen }: { chat: string; onOpen?(): void }) {
  const shares = useLinked(chat);
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  if (shares.length === 0) return <p className="px-3 py-3 text-[12.5px] text-muted-foreground">Nothing linked yet. Share messages here or from here to link conversations.</p>;
  return (
    <ul className="divide-y divide-border/50">
      {shares.map((share) => {
        const out = share.fromChat === chat;
        const other = out ? share.toChat : share.fromChat;
        const mate = mates.find((m) => m.chatId === other);
        const channel = channels.find((c) => c.chatId === other);
        const by = share.by === "person" ? "You" : (mates.find((m) => m.id === share.by)?.name ?? share.by);
        return (
          <li key={share.id}>
            <button
              type="button"
              onClick={() => {
                onOpen?.();
                void useMatesStore.getState().openChat(other);
              }}
              className="flex w-full items-center gap-2.5 px-3 py-2 text-left hover:bg-accent/60"
            >
              {mate ? (
                <MateFace avatar={mate.avatar} size={26} plate={false} label={mate.name} />
              ) : (
                <span className="flex size-[26px] shrink-0 items-center justify-center rounded-[8px] bg-muted text-muted-foreground">
                  <HashIcon className="size-3.5" />
                </span>
              )}
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="flex items-center gap-1 truncate text-[13px] font-medium text-foreground">
                  {out ? <ArrowUpRightIcon className="size-3.5 shrink-0 text-muted-foreground" /> : <ArrowDownLeftIcon className="size-3.5 shrink-0 text-muted-foreground" />}
                  {out ? "Shared into" : "Shared from"} {mate?.name ?? (channel ? `#${channel.name}` : "a conversation")}
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  {by}: {share.note || share.excerpt}
                </span>
              </span>
              <span className="shrink-0 text-[11px] text-muted-foreground">{formatRelative(share.atMs)}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

export function LinkedDialog({ chat, name, open, onOpenChange }: { chat: string; name: string; open: boolean; onOpenChange(open: boolean): void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-3 sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Linked conversations</DialogTitle>
          <DialogDescription>Where {name}'s messages were shared, and what was shared into it.</DialogDescription>
        </DialogHeader>
        <div className="-mx-2 max-h-80 overflow-y-auto">
          <LinkedList chat={chat} onOpen={() => onOpenChange(false)} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
