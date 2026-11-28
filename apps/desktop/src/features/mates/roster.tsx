// The roster (docs/features/evo-mates-ui-v2/spec.md, FR-2 – FR-6; evo-mates
// FR-6, FR-13, FR-32): rows for the full sidebar and tiles for its rail. A
// row shows the face with its presence mark, the name (bold while unread),
// a second line that says what matters first — needs you, the step running
// now, queued, paused, couldn't reply, else the last message as plain text —
// the time and an unread badge; a ⋯ button and right-click open its menu.

import * as React from "react";
import { BellIcon, BellOffIcon, HashIcon, MoreHorizontalIcon, PauseIcon, PinIcon, PinOffIcon, PlayIcon } from "lucide-react";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatRelative } from "@/lib/format";
import type { Channel, Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { ConfirmDelete } from "./confirm";
import { plainPreview } from "./plain-preview";
import { channelPresence } from "./presence";
import { MatePresenceFace, PresenceMark, usePresence } from "./presence-face";
import { exportTeam } from "./team-import";
import { currentStep, isHeartbeat, isSilent } from "./thread-rows";

/** A mate's second line, time and unread mark, from its chat (FR-4). */
export function useMateLine(mate: Mate) {
  const presence = usePresence(mate);
  const row = useSessionStore((s) => s.sessions.find((c) => c.id === mate.chatId));
  const step = useEngineStore((s) => {
    const turn = s.runtimes[mate.chatId]?.transcript.turns.at(-1);
    return turn && turn.status === "running" ? currentStep(turn) : null;
  });
  const preview = row?.preview && !isHeartbeat(row.preview) && !isSilent(row.preview) ? plainPreview(row.preview) : "";
  const line =
    presence.presence === "working" ? (step ?? presence.word ?? "Working…") : presence.presence !== "idle" ? presence.word! : preview || mate.role;
  return {
    ...presence,
    line,
    unread: Boolean(row?.unread),
    at: row?.turnCount ? row.updatedAt : null,
  };
}

/** The items of a mate's menu, for right-click and the ⋯ button (FR-6). */
function MateMenuItems({ mate, onDelete }: { mate: Mate; onDelete(): void }) {
  const store = useMatesStore.getState;
  return (
    <>
      <DropdownMenuItem onClick={() => void store().update(mate.id, { pinned: !mate.pinned })}>
        {mate.pinned ? <PinOffIcon /> : <PinIcon />}
        {mate.pinned ? "Unpin" : "Pin"}
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store().update(mate.id, { paused: !mate.paused })}>
        {mate.paused ? <PlayIcon /> : <PauseIcon />}
        {mate.paused ? "Resume" : "Pause"}
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store().update(mate.id, { notifications: !mate.notifications })}>
        {mate.notifications ? <BellOffIcon /> : <BellIcon />}
        {mate.notifications ? "Mute notifications" : "Unmute notifications"}
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => useSessionStore.getState().markUnread(mate.chatId, true)}>Mark unread</DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem
        onClick={() => {
          void store().open(mate.id);
          store().openPanel("about");
        }}
      >
        Edit profile
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store().duplicate(mate.id)}>Duplicate</DropdownMenuItem>
      <DropdownMenuItem onClick={() => void exportTeam({ mate: mate.id }, mate.name)}>Export…</DropdownMenuItem>
      <DropdownMenuItem onClick={() => void navigator.clipboard.writeText(mate.chatId)}>Copy conversation ID</DropdownMenuItem>
      <DropdownMenuItem onClick={() => void store().update(mate.id, { hidden: !mate.hidden })}>{mate.hidden ? "Show" : "Hide"}</DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem variant="destructive" onClick={onDelete}>
        Delete…
      </DropdownMenuItem>
    </>
  );
}

function useDeleteMate(mate: Mate) {
  const [confirm, setConfirm] = React.useState(false);
  const remove = useMatesStore((s) => s.remove);
  const dialog = (
    <ConfirmDelete
      open={confirm}
      onOpenChange={setConfirm}
      title={`Delete ${mate.name}?`}
      description={`${mate.name}'s conversation, memory and folder are deleted too, and ${mate.name} leaves every channel. This cannot be undone.`}
      onConfirm={() => void remove(mate.id)}
    />
  );
  return { ask: () => setConfirm(true), dialog };
}

/** The ⋯ button of a mate (rows, the header, the profile). */
export function MateMenuButton({ mate, className, label }: { mate: Mate; className?: string; label?: string }) {
  const del = useDeleteMate(mate);
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <button
              type="button"
              aria-label={label ?? `${mate.name}: more`}
              onClick={(event) => event.stopPropagation()}
              className={cn(
                "no-drag flex size-7 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground",
                className,
              )}
            />
          }
        >
          <MoreHorizontalIcon className="size-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent className="w-56" align="end">
          <MateMenuItems mate={mate} onDelete={del.ask} />
        </DropdownMenuContent>
      </DropdownMenu>
      {del.dialog}
    </>
  );
}

function WithMenu({ mate, children }: { mate: Mate; children: React.ReactElement }) {
  const del = useDeleteMate(mate);
  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger render={children} />
        <ContextMenuContent className="w-56">
          <MateMenuItems mate={mate} onDelete={del.ask} />
        </ContextMenuContent>
      </ContextMenu>
      {del.dialog}
    </>
  );
}

const ROW =
  "group/row no-drag relative flex w-full items-center gap-2.5 rounded-xl px-2 py-1.5 text-left transition-colors hover:bg-sidebar-accent/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/50";

/** One mate in the full sidebar. */
export function MateRow({ mate, active }: { mate: Mate; active: boolean }) {
  const open = useMatesStore((s) => s.open);
  const { presence, line, unread, at } = useMateLine(mate);
  const strong = unread || presence === "waiting";
  return (
    <WithMenu mate={mate}>
      <div className={cn(ROW, active && "bg-sidebar-accent hover:bg-sidebar-accent")}>
        <button
          type="button"
          onClick={() => void open(mate.id)}
          aria-current={active ? "page" : undefined}
          aria-label={`${mate.name}${line ? `, ${line}` : ""}${unread ? ", unread" : ""}`}
          className="absolute inset-0 rounded-xl focus-visible:outline-none"
        />
        <MatePresenceFace mate={mate} size={36} ring={active ? "var(--sidebar-accent)" : "var(--sidebar)"} className="pointer-events-none" />
        <span className="pointer-events-none min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span className={cn("truncate text-[13px] leading-[18px] text-sidebar-foreground", strong ? "font-semibold" : "font-medium")}>{mate.name}</span>
            {at && (
              <span className={cn("ml-auto shrink-0 text-[11px] text-muted-foreground group-hover/row:invisible", unread && "text-foreground")}>
                {formatRelative(at)}
              </span>
            )}
          </span>
          <span
            className={cn(
              "block truncate text-xs leading-4",
              presence === "waiting" ? "font-medium text-foreground" : unread ? "text-foreground/80" : "text-muted-foreground",
              presence === "working" && "text-brand",
            )}
          >
            {line}
          </span>
        </span>
        {unread && presence !== "waiting" && (
          <span aria-hidden className="size-2.5 shrink-0 rounded-full bg-brand group-hover/row:hidden" />
        )}
        <MateMenuButton mate={mate} className="absolute top-1.5 right-1.5 hidden size-6 bg-sidebar-accent group-focus-within/row:flex group-hover/row:flex" />
      </div>
    </WithMenu>
  );
}

/** A pinned mate as a tile (full sidebar). */
export function PinnedTile({ mate, active }: { mate: Mate; active: boolean }) {
  const open = useMatesStore((s) => s.open);
  const { unread } = useMateLine(mate);
  return (
    <WithMenu mate={mate}>
      <button
        type="button"
        onClick={() => void open(mate.id)}
        aria-current={active ? "page" : undefined}
        className={cn(
          "no-drag relative flex w-[60px] flex-col items-center gap-1 rounded-xl p-1 transition-colors hover:bg-sidebar-accent/70",
          active && "bg-sidebar-accent",
        )}
      >
        <MatePresenceFace mate={mate} size={40} ring={active ? "var(--sidebar-accent)" : "var(--sidebar)"} />
        <span className={cn("w-full truncate text-center text-[11px] text-sidebar-foreground", unread && "font-semibold")}>{mate.name}</span>
        {unread && <span aria-label="Unread" className="absolute top-1 right-1.5 size-2.5 rounded-full bg-brand ring-2 ring-sidebar" />}
      </button>
    </WithMenu>
  );
}

/** A mate in the rail: its face, its mark, its unread badge; its name on hover (FR-2). */
export function MateTile({ mate, active }: { mate: Mate; active: boolean }) {
  const open = useMatesStore((s) => s.open);
  const { line, unread } = useMateLine(mate);
  return (
    <Tooltip>
      <WithMenu mate={mate}>
        <TooltipTrigger
          render={
            <button
              type="button"
              onClick={() => void open(mate.id)}
              aria-current={active ? "page" : undefined}
              aria-label={`${mate.name}${unread ? ", unread" : ""}`}
              className={cn(
                "no-drag relative flex size-12 items-center justify-center rounded-2xl transition-colors hover:bg-sidebar-accent/70",
                active && "bg-sidebar-accent",
              )}
            />
          }
        >
          {active && <span aria-hidden className="absolute top-2 bottom-2 -left-2.5 w-1 rounded-r-full bg-foreground" />}
          <MatePresenceFace mate={mate} size={36} ring={active ? "var(--sidebar-accent)" : "var(--sidebar)"} />
          {unread && <span aria-hidden className="absolute top-1 right-1 size-2.5 rounded-full bg-brand ring-2 ring-sidebar" />}
        </TooltipTrigger>
      </WithMenu>
      <TooltipContent side="right">
        <span className="font-medium">{mate.name}</span>
        {line && <span className="opacity-70">· {line}</span>}
      </TooltipContent>
    </Tooltip>
  );
}

/** A channel's state, line and unread mark (FR-4, C3). */
export function useChannelLine(channel: Channel) {
  const work = useSessionStore((s) => s.work);
  const row = useSessionStore((s) => s.sessions.find((c) => c.id === channel.chatId));
  const mates = useMatesStore((s) => s.mates);
  const { working, waiting, presence } = channelPresence(channel.members, work, work[channel.chatId]);
  const nameOf = (id: string) => mates.find((m) => m.id === id)?.name ?? "A mate";
  const line =
    presence === "waiting"
      ? waiting.length
        ? `${nameOf(waiting[0])} needs you`
        : "Needs you"
      : presence === "working"
        ? working.length === 1
          ? `${nameOf(working[0])} is working…`
          : working.length > 1
            ? `${working.length} mates are working…`
            : "Working…"
        : plainPreview(row?.preview) || `${channel.members.length} ${channel.members.length === 1 ? "mate" : "mates"}`;
  return { presence, line, unread: Boolean(row?.unread), at: row?.turnCount ? row.updatedAt : null, working };
}

/** A channel's square glyph: # on a soft tile, with its presence mark. */
export function ChannelGlyph({ channel, size = 36, presence, ring }: { channel: Channel; size?: number; presence: ReturnType<typeof channelPresence>["presence"]; ring?: string }) {
  return (
    <span className="relative inline-flex shrink-0" style={{ "--presence-ring": ring ?? "var(--sidebar)" } as React.CSSProperties}>
      <span
        className="flex items-center justify-center rounded-[10px] bg-foreground/[0.06] text-muted-foreground dark:bg-foreground/10"
        style={{ width: size, height: size }}
        aria-hidden
      >
        <HashIcon style={{ width: size * 0.45, height: size * 0.45 }} />
      </span>
      <PresenceMark presence={presence} size={size} />
      <span className="sr-only">{channel.name}</span>
    </span>
  );
}

export function ChannelRow({ channel, active }: { channel: Channel; active: boolean }) {
  const openChannel = useMatesStore((s) => s.openChannel);
  const { presence, line, unread, at } = useChannelLine(channel);
  const strong = unread || presence === "waiting";
  return (
    <div className={cn(ROW, active && "bg-sidebar-accent hover:bg-sidebar-accent")}>
      <button
        type="button"
        onClick={() => void openChannel(channel.id)}
        aria-current={active ? "page" : undefined}
        aria-label={`#${channel.name}, ${line}${unread ? ", unread" : ""}`}
        className="absolute inset-0 rounded-xl focus-visible:outline-none"
      />
      <ChannelGlyph channel={channel} presence={presence} ring={active ? "var(--sidebar-accent)" : "var(--sidebar)"} />
      <span className="pointer-events-none min-w-0 flex-1">
        <span className="flex items-baseline gap-2">
          <span className={cn("truncate text-[13px] leading-[18px] text-sidebar-foreground", strong ? "font-semibold" : "font-medium")}>{channel.name}</span>
          {at && <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">{formatRelative(at)}</span>}
        </span>
        <span
          className={cn(
            "block truncate text-xs leading-4",
            presence === "waiting" ? "font-medium text-foreground" : presence === "working" ? "text-brand" : unread ? "text-foreground/80" : "text-muted-foreground",
          )}
        >
          {line}
        </span>
      </span>
      {unread && presence !== "waiting" && <span aria-hidden className="size-2.5 shrink-0 rounded-full bg-brand" />}
    </div>
  );
}

export function ChannelTile({ channel, active }: { channel: Channel; active: boolean }) {
  const openChannel = useMatesStore((s) => s.openChannel);
  const { presence, line, unread } = useChannelLine(channel);
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            onClick={() => void openChannel(channel.id)}
            aria-current={active ? "page" : undefined}
            aria-label={`#${channel.name}${unread ? ", unread" : ""}`}
            className={cn(
              "no-drag relative flex size-12 items-center justify-center rounded-2xl transition-colors hover:bg-sidebar-accent/70",
              active && "bg-sidebar-accent",
            )}
          />
        }
      >
        {active && <span aria-hidden className="absolute top-2 bottom-2 -left-2.5 w-1 rounded-r-full bg-foreground" />}
        <ChannelGlyph channel={channel} presence={presence} ring={active ? "var(--sidebar-accent)" : "var(--sidebar)"} />
        {unread && <span aria-hidden className="absolute top-1 right-1 size-2.5 rounded-full bg-brand ring-2 ring-sidebar" />}
      </TooltipTrigger>
      <TooltipContent side="right">
        <span className="font-medium">#{channel.name}</span>
        <span className="opacity-70">· {line}</span>
      </TooltipContent>
    </Tooltip>
  );
}
