// Who is typing in a channel (docs/features/evo-mates-messaging/spec.md,
// FR-1): one line at the feed's foot with the working mates' faces and
// moving dots — "Writer is typing…", "Writer and Analyst are typing…",
// "Planner and 2 others are typing…". A face's tooltip says its step, and
// Stop for that mate. Reduced motion holds the dots still.

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { channelsApi, type Channel, type Mate } from "@/lib/mates";
import { useEngineStore } from "@/stores/engine-store";
import { typingWords } from "./composer-keys";
import { PresenceFace } from "./presence-face";
import { currentStep } from "./thread-rows";

function TypingFace({ channel, mate, seat }: { channel: Channel; mate: Mate; seat: string | null }) {
  // The seat's step, when its run is loaded in this window.
  const step = useEngineStore((s) => {
    const turn = seat ? s.runtimes[seat]?.transcript.turns.at(-1) : undefined;
    return turn && turn.status === "running" ? currentStep(turn) : null;
  });
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            aria-label={`${mate.name}: ${step ?? "Working…"} Stop ${mate.name}`}
            onClick={() => void channelsApi.stopMember(channel.id, mate.id).catch(() => undefined)}
            className="rounded-full ring-2 ring-background"
          />
        }
      >
        <PresenceFace avatar={mate.avatar} size={24} mood="working" presence="working" live label={mate.name} ring="var(--background)" />
      </TooltipTrigger>
      <TooltipContent>
        <span className="font-medium">{mate.name}</span> · {step ?? "Working…"} <span className="opacity-70">· click to stop</span>
      </TooltipContent>
    </Tooltip>
  );
}

export function TypingLine({ channel, working }: { channel: Channel; working: Mate[] }) {
  if (working.length === 0) return null;
  const seatOf = (mate: Mate) => channel.members.find((m) => m.mateId === mate.id)?.seatChatId ?? null;
  return (
    <div className="mt-2 flex items-center gap-2 px-4" aria-live="polite">
      <span className="flex -space-x-1.5">
        {working.slice(0, 3).map((mate) => (
          <TypingFace key={mate.id} channel={channel} mate={mate} seat={seatOf(mate)} />
        ))}
      </span>
      <span className="flex items-center gap-1 rounded-full bg-muted px-2.5 py-1.5" aria-hidden>
        {[0, 150, 300].map((delay) => (
          <span
            key={delay}
            className="size-1.5 rounded-full bg-muted-foreground/70 motion-safe:animate-bounce"
            style={{ animationDelay: `${delay}ms`, animationDuration: "1s" }}
          />
        ))}
      </span>
      <span className="truncate text-xs text-muted-foreground">{typingWords(working.map((m) => m.name))}</span>
    </div>
  );
}
