// A mate's initiative and its Following up list (docs/features/evo-mates/
// spec.md, FR-65, FR-69, FR-70): how often it may write first, and what it
// plans to come back to, each item removable, "never", or a check-in that
// can be paused.

import * as React from "react";
import { BellRingIcon, CalendarClockIcon, CheckIcon, MessageCircleQuestionIcon, PauseIcon, PlayIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { matesApi, type AgendaItem, type Initiative, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";

const LEVELS: { id: Initiative; label: string; hint: string }[] = [
  { id: "medium", label: "Medium", hint: "Up to 5 messages a day." },
  { id: "low", label: "Low", hint: "Up to 2 a day." },
  { id: "off", label: "Off", hint: "Only answers you." },
];

const KIND_ICON = {
  promise: CheckIcon,
  event: CalendarClockIcon,
  question: MessageCircleQuestionIcon,
  checkin: BellRingIcon,
} as const;

/** "Today 11:00", "Tomorrow 09:00", "Fri 28 Nov 09:00". */
export function dueWords(at: number, now = Date.now()): string {
  const date = new Date(at);
  const time = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(date) - day(new Date(now))) / 86_400_000);
  if (days === 0) return `Today ${time}`;
  if (days === 1) return `Tomorrow ${time}`;
  if (days === -1) return `Yesterday ${time}`;
  return `${date.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} ${time}`;
}

export function FollowingUp({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const mates = useMatesStore((s) => s.mates);
  const [items, setItems] = React.useState<AgendaItem[]>([]);
  const [repeats, setRepeats] = React.useState<Record<string, string>>({});

  // Read again whenever the mates change (`mates://changed`: an item added or fired).
  React.useEffect(() => {
    let gone = false;
    void matesApi
      .agenda(mate.id)
      .then(async (list) => {
        if (gone) return;
        setItems(list);
        const words: Record<string, string> = {};
        for (const item of list) {
          if (item.repeat) words[item.id] = (await matesApi.repeatWords(item.repeat).catch(() => null)) ?? "";
        }
        if (!gone) setRepeats(words);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [mate.id, mates]);

  const set = (item: AgendaItem, status: "done" | "never" | "paused" | "open") => {
    setItems((current) =>
      status === "done" || status === "never" ? current.filter((i) => i.id !== item.id) : current.map((i) => (i.id === item.id ? { ...i, status } : i)),
    );
    void matesApi.setAgenda(mate.id, item.id, status).catch(() => undefined);
  };

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1.5" role="radiogroup" aria-label="Initiative">
        <span className="text-xs font-medium text-muted-foreground">Writes to you first</span>
        <div className="grid grid-cols-3 gap-1.5">
          {LEVELS.map((level) => (
            <button
              key={level.id}
              type="button"
              role="radio"
              aria-checked={mate.initiative === level.id}
              onClick={() => void update(mate.id, { initiative: level.id })}
              className={cn(
                "flex flex-col items-start rounded-2xl bg-muted/40 px-3 py-2 text-left transition-colors hover:bg-muted/80",
                mate.initiative === level.id && "bg-brand/5 shadow-[0_0_0_1.5px_var(--brand)]",
              )}
            >
              <span className="text-[13px] font-medium text-foreground">{level.label}</span>
              <span className="text-[11px] leading-4 text-muted-foreground">{level.hint}</span>
            </button>
          ))}
        </div>
        <p className="text-xs text-muted-foreground">
          {mate.name} follows up on what you talked about, at most once an hour, never while Do not disturb is on, and stops after two
          messages you did not answer.
        </p>
      </div>
      <section className="flex flex-col gap-1.5">
        <h3 className="text-xs font-medium text-muted-foreground">Following up</h3>
        {items.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Nothing yet. When you mention something coming up, or ask {mate.name} for a regular check-in, it shows here.
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {items.map((item) => {
              const Icon = KIND_ICON[item.kind] ?? CheckIcon;
              const paused = item.status === "paused";
              return (
                <li key={item.id} className={cn("flex items-start gap-2 rounded-2xl bg-muted/40 px-3 py-2", paused && "opacity-60")}>
                  <Icon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] leading-5 text-foreground">{item.text}</div>
                    <div className="text-[11px] text-muted-foreground">
                      {paused ? "Paused" : item.repeat ? (repeats[item.id] || "Repeats") : dueWords(item.dueAt)}
                    </div>
                    <div className="mt-1 flex gap-1">
                      {item.kind === "checkin" && (
                        <Button size="xs" variant="ghost" className="h-6 rounded-full px-2 text-[11px]" onClick={() => set(item, paused ? "open" : "paused")}>
                          {paused ? <PlayIcon /> : <PauseIcon />}
                          {paused ? "Resume" : "Pause"}
                        </Button>
                      )}
                      <Button size="xs" variant="ghost" className="h-6 rounded-full px-2 text-[11px]" onClick={() => set(item, "done")}>
                        Remove
                      </Button>
                      {item.kind !== "checkin" && (
                        <Button size="xs" variant="ghost" className="h-6 rounded-full px-2 text-[11px] text-muted-foreground" onClick={() => set(item, "never")}>
                          Never follow up
                        </Button>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
