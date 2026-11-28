// Mute a mate or a channel for a while, and a channel's notification level
// (docs/features/evo-mates-messaging/spec.md, FR-15). Kept in the app
// setting the shell reads before a notice.

import { BellIcon, BellOffIcon, CheckIcon } from "lucide-react";
import { DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { isQuiet, muteUntil, useMatesPrefs, type Level } from "@/stores/mates-prefs-store";

const LEVELS: { id: Level; label: string }[] = [
  { id: "all", label: "Everything" },
  { id: "mentions", label: "Mentions and Needs you" },
  { id: "nothing", label: "Nothing" },
];

export function MuteMenuItems({ chat, levels = false }: { chat: string; levels?: boolean }) {
  const mute = useMatesPrefs((s) => s.mute[chat]);
  const set = (until: number | null | undefined) => useMatesPrefs.getState().setMute(chat, { ...mute, until });
  const muted = mute?.until === null || (typeof mute?.until === "number" && mute.until > Date.now());
  return (
    <>
      {/* A menu label lives in a group (Base UI's Menu.Group): without it the menu fails to open. */}
      <DropdownMenuGroup>
      <DropdownMenuLabel className="text-xs text-muted-foreground">Notifications</DropdownMenuLabel>
      {muted ? (
        <DropdownMenuItem onClick={() => set(undefined)}>
          <BellIcon />
          Unmute
        </DropdownMenuItem>
      ) : (
        <>
          <DropdownMenuItem onClick={() => set(muteUntil("hour"))}>
            <BellOffIcon />
            Mute for 1 hour
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => set(muteUntil("8h"))}>
            <BellOffIcon />
            Mute for 8 hours
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => set(muteUntil("tomorrow"))}>
            <BellOffIcon />
            Mute until tomorrow 8:00
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => set(muteUntil("always"))}>
            <BellOffIcon />
            Mute until I turn it back on
          </DropdownMenuItem>
        </>
      )}
      </DropdownMenuGroup>
      {levels && (
        <DropdownMenuGroup>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs text-muted-foreground">Notify me about</DropdownMenuLabel>
          {LEVELS.map((level) => (
            <DropdownMenuItem key={level.id} onClick={() => useMatesPrefs.getState().setMute(chat, { ...mute, level: level.id })}>
              {(mute?.level ?? "all") === level.id ? <CheckIcon /> : <span className="size-4" />}
              {level.label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
      )}
    </>
  );
}

/** A muted or quiet place's mark in its row (FR-15). */
export function QuietMark({ chat }: { chat: string }) {
  const quiet = useMatesPrefs((s) => isQuiet(s.mute[chat]));
  if (!quiet) return null;
  return <BellOffIcon aria-label="Muted" className="size-3 shrink-0 text-muted-foreground" />;
}
