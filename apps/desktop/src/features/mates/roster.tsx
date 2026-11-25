// The roster (docs/features/evo-mates/ux.md 2; spec FR-6, FR-13, FR-32):
// pinned mates as tiles, then every mate as a row with its face, name, one
// line (the last message or what it is doing) and time; right-click for its
// menu. Hidden mates fold under "Hidden".

import * as React from "react";
import { ChevronRightIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { formatRelative } from "@/lib/format";
import type { Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { ConfirmDelete } from "./confirm";
import { useEngineStore } from "@/stores/engine-store";
import { mateMood, moodWord, useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";

/** A mate's state, last line and time, from its chat. */
function useMateLine(mate: Mate) {
  const work = useSessionStore((s) => s.work[mate.chatId]);
  const row = useSessionStore((s) => s.sessions.find((c) => c.id === mate.chatId));
  const busy = useEngineStore((s) => s.runtimes[mate.chatId]?.transcript.busy ?? false);
  const mood = mateMood(work, busy);
  return {
    mood,
    line: moodWord(mood) ?? row?.preview ?? mate.role,
    unread: Boolean(row?.unread),
    at: row?.turnCount ? row.updatedAt : null,
  };
}

function MateMenu({ mate, onDelete }: { mate: Mate; onDelete(): void }) {
  const store = useMatesStore();
  return (
    <ContextMenuContent className="w-52">
      <DropdownMenuItem onClick={() => void store.update(mate.id, { pinned: !mate.pinned })}>
        {mate.pinned ? "Unpin" : "Pin"}
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => useSessionStore.getState().markUnread(mate.chatId, true)}>Mark unread</DropdownMenuItem>
      <DropdownMenuItem
        onClick={() => {
          void store.open(mate.id);
          store.setPanelOpen(true);
        }}
      >
        Edit profile
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store.duplicate(mate.id)}>Duplicate</DropdownMenuItem>
      <DropdownMenuItem onClick={() => void navigator.clipboard.writeText(mate.chatId)}>
        Copy conversation ID
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store.update(mate.id, { hidden: !mate.hidden })}>
        {mate.hidden ? "Show" : "Hide"}
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem variant="destructive" onClick={onDelete}>
        Delete…
      </DropdownMenuItem>
    </ContextMenuContent>
  );
}

function WithMenu({ mate, children }: { mate: Mate; children: React.ReactElement }) {
  const [confirm, setConfirm] = React.useState(false);
  const remove = useMatesStore((s) => s.remove);
  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger render={children} />
        <MateMenu mate={mate} onDelete={() => setConfirm(true)} />
      </ContextMenu>
      <ConfirmDelete
        open={confirm}
        onOpenChange={setConfirm}
        title={`Delete ${mate.name}?`}
        description={`${mate.name}'s conversation, memory and folder are deleted too, and ${mate.name} leaves every channel. This cannot be undone.`}
        onConfirm={() => void remove(mate.id)}
      />
    </>
  );
}

function MateRow({ mate, active }: { mate: Mate; active: boolean }) {
  const open = useMatesStore((s) => s.open);
  const { mood, line, unread, at } = useMateLine(mate);
  return (
    <WithMenu mate={mate}>
      <button
        type="button"
        onClick={() => void open(mate.id)}
        className={cn(
          "no-drag flex w-full items-center gap-2.5 rounded-xl p-2 text-left transition-colors hover:bg-sidebar-accent",
          active && "bg-sidebar-accent",
        )}
      >
        <MateFace avatar={mate.avatar} size={36} mood={mood} label={mate.name} />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className="truncate text-[13px] leading-[18px] font-medium text-sidebar-foreground">{mate.name}</span>
            {at && <span className="ml-auto shrink-0 text-xs text-muted-foreground">{formatRelative(at)}</span>}
          </span>
          <span
            className={cn(
              "block truncate text-xs leading-4 text-muted-foreground",
              mood === "waiting" && "text-foreground",
            )}
          >
            {line}
          </span>
        </span>
        {(unread || mood === "waiting") && (
          <span aria-label={mood === "waiting" ? "Needs you" : "Unread"} className="size-2 shrink-0 rounded-full bg-primary" />
        )}
      </button>
    </WithMenu>
  );
}

function PinnedTile({ mate, active }: { mate: Mate; active: boolean }) {
  const open = useMatesStore((s) => s.open);
  const { mood } = useMateLine(mate);
  return (
    <WithMenu mate={mate}>
      <button
        type="button"
        onClick={() => void open(mate.id)}
        className={cn(
          "no-drag flex w-[78px] flex-col items-center gap-1 rounded-xl p-1.5 transition-colors hover:bg-sidebar-accent",
          active && "bg-sidebar-accent",
        )}
      >
        <MateFace avatar={mate.avatar} size={52} mood={mood} label={mate.name} />
        <span className="w-full truncate text-center text-xs text-sidebar-foreground">{mate.name}</span>
      </button>
    </WithMenu>
  );
}

export function Roster({ query }: { query: string }) {
  const mates = useMatesStore((s) => s.mates);
  const view = useMatesStore((s) => s.view);
  const [showHidden, setShowHidden] = React.useState(false);
  const active = view.kind === "mate" ? view.id : null;
  const needle = query.trim().toLowerCase();
  const matches = (mate: Mate) =>
    !needle || mate.name.toLowerCase().includes(needle) || mate.role.toLowerCase().includes(needle);
  const shown = mates.filter((m) => !m.hidden && matches(m));
  const pinned = needle ? [] : shown.filter((m) => m.pinned);
  const hidden = mates.filter((m) => m.hidden && matches(m));
  return (
    <>
      {pinned.length > 0 && (
        <div className="flex flex-wrap gap-1 px-2 pt-2">
          {pinned.map((mate) => (
            <PinnedTile key={mate.id} mate={mate} active={mate.id === active} />
          ))}
        </div>
      )}
      <section className="px-2 pt-3">
        <h2 className="px-2 pb-1 text-xs text-muted-foreground">Mates</h2>
        {shown.length === 0 ? (
          <p className="px-2 py-1 text-[13px] text-muted-foreground/80">{needle ? "No mate matches" : "No mates yet"}</p>
        ) : (
          shown.filter((m) => needle || !m.pinned).map((mate) => <MateRow key={mate.id} mate={mate} active={mate.id === active} />)
        )}
      </section>
      {hidden.length > 0 && (
        <section className="px-2 pt-2">
          <button
            type="button"
            onClick={() => setShowHidden(!showHidden)}
            className="flex items-center gap-1 px-2 pb-1 text-xs text-muted-foreground"
          >
            <ChevronRightIcon className={cn("size-3 transition-transform", showHidden && "rotate-90")} />
            Hidden ({hidden.length})
          </button>
          {showHidden && hidden.map((mate) => <MateRow key={mate.id} mate={mate} active={mate.id === active} />)}
        </section>
      )}
    </>
  );
}
