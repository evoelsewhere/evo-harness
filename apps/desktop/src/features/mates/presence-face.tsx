// A mate's face with its presence mark (docs/features/evo-mates-ui-v2/spec.md,
// FR-5, NFR-3, NFR-6): a dot at the face's corner — working (accent, a soft
// pulse unless reduced motion), needs you (amber), couldn't reply (red),
// queued or paused (hollow grey). Idle has none.

import * as React from "react";
import { MateFace, type MateAvatar, type MateMood } from "@/components/mate-face/mate-face";
import type { Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useEngineStore } from "@/stores/engine-store";
import { mateMood, useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { presenceLabel, presenceOf, type Presence, type PresenceInfo } from "./presence";

/** A mate's mood and presence, from its chat (FR-4, FR-5). */
export function usePresence(mate: Mate | undefined | null): PresenceInfo & { mood: MateMood } {
  const chat = mate?.chatId ?? "";
  const work = useSessionStore((s) => (chat ? s.work[chat] : undefined));
  const busy = useEngineStore((s) => (chat ? (s.runtimes[chat]?.transcript.busy ?? false) : false));
  const failed = useEngineStore((s) => (chat ? s.runtimes[chat]?.transcript.turns.at(-1)?.status === "error" : false));
  const queued = useMatesStore((s) => (chat ? s.queued.includes(chat) : false));
  const mood = mateMood(work, busy, queued, failed);
  return { mood, ...presenceOf(mood, Boolean(mate?.paused)) };
}

/** The corner mark alone, for a face drawn elsewhere (a channel's tile). */
export function PresenceMark({ presence, size, className }: { presence: Presence; size: number; className?: string }) {
  if (presence === "idle") return null;
  const dot = Math.max(8, Math.round(size * 0.28));
  const label = presenceLabel(presence) ?? undefined;
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={cn(
        "absolute right-0 bottom-0 rounded-full ring-2 ring-(--presence-ring,var(--sidebar))",
        presence === "working" && "bg-brand motion-safe:animate-[presence-pulse_1.8s_ease-in-out_infinite]",
        presence === "waiting" && "bg-amber-500",
        presence === "error" && "bg-red-500",
        (presence === "queued" || presence === "paused") && "bg-(--presence-ring,var(--sidebar)) shadow-[inset_0_0_0_2px_var(--muted-foreground)]",
        className,
      )}
      style={{ width: dot, height: dot }}
    />
  );
}

/**
 * `MateFace` with its presence mark. `ring` is the surface colour behind
 * the face, so the mark's outline matches it (sidebar, card, background).
 */
export function PresenceFace({
  avatar,
  size,
  mood,
  presence,
  live = false,
  label,
  plate = false,
  ring = "var(--sidebar)",
  className,
}: {
  avatar: MateAvatar | null | undefined;
  size: number;
  mood?: MateMood;
  presence: Presence;
  live?: boolean;
  label?: string;
  plate?: boolean;
  ring?: string;
  className?: string;
}) {
  return (
    <span
      className={cn("relative inline-flex shrink-0", presence === "queued" && "opacity-60", className)}
      style={{ "--presence-ring": ring } as React.CSSProperties}
    >
      <MateFace avatar={avatar} size={size} mood={mood} live={live} label={label} plate={plate} />
      <PresenceMark presence={presence} size={size} />
    </span>
  );
}

/** A mate's face with the mark from its own state. */
export function MatePresenceFace({
  mate,
  size,
  live,
  ring,
  plate,
  className,
}: {
  mate: Mate;
  size: number;
  live?: boolean;
  ring?: string;
  plate?: boolean;
  className?: string;
}) {
  const { mood, presence } = usePresence(mate);
  return (
    <PresenceFace
      avatar={mate.avatar}
      size={size}
      mood={mood}
      presence={presence}
      live={live}
      label={mate.name}
      ring={ring}
      plate={plate}
      className={className}
    />
  );
}
