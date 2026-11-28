// A channel's header (docs/features/evo-mates-ui-v2/spec.md, FR-19; evo-mates
// ux.md 5, QA C7): its name and the first line of its instructions, the
// members' faces with their presence (a click lists them with role and
// state), the reply rule as a chip that changes it, Stop while anyone works,
// Find, and a ⋯ menu for Run retro, Export, Settings and Delete.

import * as React from "react";
import { ChevronDownIcon, ClipboardCheckIcon, CrownIcon, DownloadIcon, HashIcon, MoreHorizontalIcon, SearchIcon, SettingsIcon, SquareIcon, Trash2Icon, UsersIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { log } from "@/lib/db";
import { channelsApi, type Channel, type Mate, type ReplyRule } from "@/lib/mates";
import { modKey } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { retroFailure } from "./channel-rows";
import { ChannelSettings, RULES } from "./channel-settings";
import { ConfirmDelete } from "./confirm";
import { HeaderButton } from "./mate-header";
import { channelPresence } from "./presence";
import { PresenceFace, usePresence } from "./presence-face";
import { exportTeam } from "./team-import";
import { MuteMenuItems } from "./mute-menu";

export function useMembers(channel: Channel): Mate[] {
  const mates = useMatesStore((s) => s.mates);
  return React.useMemo(
    () => channel.members.map((m) => mates.find((mate) => mate.id === m.mateId)).filter((m): m is Mate => Boolean(m)),
    [channel.members, mates],
  );
}

/** One member in the list: its face with its seat's state, role, and Message. */
function MemberRow({ channel, mate, seatState }: { channel: Channel; mate: Mate; seatState: "working" | "waiting" | undefined }) {
  const own = usePresence(mate);
  // In the channel, the seat's state counts; else the mate's own.
  const presence = seatState ?? (own.presence === "paused" ? "paused" : "idle");
  const word = seatState === "working" ? "Working here…" : seatState === "waiting" ? "Needs you here" : mate.paused ? "Paused" : "Free";
  return (
    <li className="flex items-center gap-2.5 rounded-xl px-2 py-1.5 hover:bg-accent/60">
      <PresenceFace avatar={mate.avatar} size={32} presence={presence} label={mate.name} ring="var(--popover)" mood={seatState === "working" ? "working" : undefined} />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
          <span className="truncate">{mate.name}</span>
          {channel.leadMateId === mate.id && (
            <span className="flex items-center gap-0.5 rounded-full bg-muted px-1.5 text-[10.5px] font-medium text-muted-foreground">
              <CrownIcon className="size-2.5" />
              Lead
            </span>
          )}
        </span>
        <span className="block truncate text-xs text-muted-foreground">
          {mate.role || "Mate"} · <span className={cn(seatState === "working" && "text-brand", seatState === "waiting" && "font-medium text-foreground")}>{word}</span>
        </span>
      </span>
      <Button size="sm" variant="ghost" className="h-7 rounded-full px-2.5 text-xs" onClick={() => void useMatesStore.getState().open(mate.id)}>
        Message
      </Button>
    </li>
  );
}

export function ChannelHeader({ channel }: { channel: Channel }) {
  const members = useMembers(channel);
  const removeChannel = useMatesStore((s) => s.removeChannel);
  const updateChannel = useMatesStore((s) => s.updateChannel);
  const work = useSessionStore((s) => s.work);
  const busy = useEngineStore((s) => s.activeId === channel.chatId && s.transcript.busy);
  const membersOpen = useMatesUiStore((s) => s.membersOpen);
  const setMembersOpen = useMatesUiStore((s) => s.setMembersOpen);
  const findOpen = useMatesUiStore((s) => s.findOpen);
  const [settings, setSettings] = React.useState(false);
  const [confirm, setConfirm] = React.useState(false);
  // Why Run retro did not run, for a few seconds (learning-layers FR-19).
  const [retroNote, setRetroNote] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!retroNote) return;
    const timer = window.setTimeout(() => setRetroNote(null), 8000);
    return () => window.clearTimeout(timer);
  }, [retroNote]);
  React.useEffect(() => () => setMembersOpen(false), [channel.id, setMembersOpen]);
  const { working } = channelPresence(channel.members, work, work[channel.chatId]);
  const anyWorking = busy || working.length > 0;
  const rule = RULES.find((r) => r.id === channel.replyRule);
  const seatState = (mateId: string) => {
    const seat = channel.members.find((m) => m.mateId === mateId)?.seatChatId;
    return seat ? work[seat] : undefined;
  };
  const firstLine = channel.instructions.split("\n").find((l) => l.trim())?.trim() ?? "";

  return (
    <div className="flex min-w-0 flex-1 items-center gap-2 pr-2 pl-4">
      <button
        type="button"
        onClick={() => setSettings(true)}
        aria-label={`#${channel.name}: settings`}
        className="no-drag flex min-w-0 items-center gap-2.5 rounded-xl py-1 pr-2 pl-1 text-left transition-colors hover:bg-accent/60"
      >
        <span className="flex size-8 shrink-0 items-center justify-center rounded-[10px] bg-foreground/[0.06] text-muted-foreground dark:bg-foreground/10">
          <HashIcon className="size-4" />
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="truncate text-[14px] leading-[18px] font-semibold text-foreground">{channel.name}</span>
          <span className={cn("truncate text-xs leading-4", anyWorking ? "text-brand" : "text-muted-foreground")}>
            {anyWorking
              ? working.length > 1
                ? `${working.length} mates are working…`
                : `${members.find((m) => m.id === working[0])?.name ?? "A mate"} is working…`
              : firstLine || `${members.length} ${members.length === 1 ? "mate" : "mates"}`}
          </span>
        </span>
      </button>
      {/* The members, with their state (FR-19). */}
      <Popover open={membersOpen} onOpenChange={setMembersOpen}>
        <PopoverTrigger
          render={
            <button
              type="button"
              aria-label={`Members (${modKey} I)`}
              className="no-drag flex h-8 shrink-0 items-center gap-1 rounded-full pr-2 pl-1 transition-colors hover:bg-accent/60"
            />
          }
        >
          <span className="flex -space-x-1.5">
            {members.slice(0, 4).map((mate) => {
              const state = seatState(mate.id);
              return (
                <PresenceFace
                  key={mate.id}
                  avatar={mate.avatar}
                  size={24}
                  presence={state ?? "idle"}
                  label={mate.name}
                  ring="var(--background)"
                  className="rounded-full ring-2 ring-background"
                />
              );
            })}
          </span>
          {members.length > 4 && <span className="text-xs text-muted-foreground">+{members.length - 4}</span>}
          <UsersIcon className="ml-0.5 size-3.5 text-muted-foreground" />
        </PopoverTrigger>
        <PopoverContent className="w-80 p-1.5" align="start">
          <div className="px-2 pt-1 pb-1.5 text-xs font-medium text-muted-foreground">
            {members.length} {members.length === 1 ? "member" : "members"}
          </div>
          <ul className="flex flex-col">
            {members.map((mate) => (
              <MemberRow key={mate.id} channel={channel} mate={mate} seatState={seatState(mate.id)} />
            ))}
          </ul>
        </PopoverContent>
      </Popover>
      {/* The reply rule, changed from its chip (FR-19). */}
      {rule && (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <button
                type="button"
                title={rule.hint}
                className="no-drag hidden h-7 shrink-0 items-center gap-1 rounded-full bg-muted px-2.5 text-[11.5px] font-medium text-muted-foreground transition-colors hover:text-foreground min-[900px]:flex"
              />
            }
          >
            {rule.label}
            <ChevronDownIcon className="size-3" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-64">
            <DropdownMenuRadioGroup value={channel.replyRule} onValueChange={(value) => void updateChannel(channel.id, { replyRule: value as ReplyRule })}>
              {RULES.map((r) => (
                <DropdownMenuRadioItem key={r.id} value={r.id} className="items-start">
                  <span className="flex flex-col">
                    <span className="text-[13px]">{r.label}</span>
                    <span className="text-xs text-muted-foreground">{r.hint}</span>
                  </span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <div className="flex-1" />
      {retroNote && (
        <span role="status" className="max-w-[35%] truncate text-xs text-muted-foreground" title={retroNote}>
          {retroNote}
        </span>
      )}
      <div className="flex shrink-0 items-center gap-0.5">
        {anyWorking && (
          <Button
            size="sm"
            variant="secondary"
            className="no-drag mr-1 h-7 gap-1.5 rounded-full px-2.5 text-xs"
            onClick={() => void useEngineStore.getState().abort()}
            title="Stop every mate in this channel"
          >
            <SquareIcon className="size-2.5 fill-current" />
            Stop
          </Button>
        )}
        <HeaderButton label={`Find (${modKey} F)`} active={findOpen} onClick={() => useMatesUiStore.getState().setFindOpen(!findOpen)}>
          <SearchIcon />
        </HeaderButton>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <button
                type="button"
                aria-label={`#${channel.name}: more`}
                className="no-drag flex size-8 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
              />
            }
          >
            <MoreHorizontalIcon className="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-60" align="end">
            <DropdownMenuItem
              onClick={() => {
                setRetroNote(null);
                void channelsApi.runRetro(channel.chatId).catch((error) => {
                  log("warn", "mates", `the retro did not run: ${String(error)}`);
                  setRetroNote(retroFailure(String(error)));
                });
              }}
            >
              <ClipboardCheckIcon />
              Run retro
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => void exportTeam({ channel: channel.id }, channel.name)}>
              <DownloadIcon />
              Export channel…
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSettings(true)}>
              <SettingsIcon />
              Channel settings
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <MuteMenuItems chat={channel.chatId} levels />
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onClick={() => setConfirm(true)}>
              <Trash2Icon />
              Delete channel…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <ChannelSettings open={settings} onOpenChange={setSettings} channel={channel} />
      <ConfirmDelete
        open={confirm}
        onOpenChange={setConfirm}
        title={`Delete ${channel.name}?`}
        description="Its messages go. The mates stay, with their own conversations and memory."
        onConfirm={() => void removeChannel(channel.id)}
      />
    </div>
  );
}
