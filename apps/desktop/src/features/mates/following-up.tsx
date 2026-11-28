// A mate's initiative and its Following up list (docs/features/evo-mates/
// spec.md, FR-65, FR-69, FR-70): how often it may write first, and what it
// plans to come back to, each item removable, "never", or a check-in that
// can be paused.

import * as React from "react";
import { BellRingIcon, CalendarClockIcon, CheckIcon, MessageCircleQuestionIcon, PauseIcon, PlayIcon, PlusIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { matesApi, type AgendaItem, type CheckinSchedule, type Initiative, type Mate } from "@/lib/mates";
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

/** Add check-in's presets (FR-79). */
export const CHECKIN_PRESETS: { label: string; text: string; schedule: CheckinSchedule }[] = [
  { label: "Morning brief", text: "Brief me on today: what is due, what is open, what I should look at first", schedule: { kind: "weekdays", time: "09:00" } },
  { label: "End of day", text: "Sum up what we did today and what is left for tomorrow", schedule: { kind: "weekdays", time: "17:30" } },
  { label: "Weekly review", text: "Review the week: what got done, what slipped, what to plan next", schedule: { kind: "weekly", time: "16:00", days: [5] } },
];

/** Add a check-in without asking in chat (FR-79). */
function AddCheckin({ mate }: { mate: Mate }) {
  const [open, setOpen] = React.useState(false);
  const [text, setText] = React.useState("");
  const [schedule, setSchedule] = React.useState<CheckinSchedule>({ kind: "weekdays", time: "09:00", days: [1] });
  const [error, setError] = React.useState<string | null>(null);
  if (!open)
    return (
      <Button size="xs" variant="secondary" className="h-7 self-start rounded-full px-3 text-xs" onClick={() => setOpen(true)}>
        <PlusIcon />
        Add check-in
      </Button>
    );
  return (
    <div className="flex flex-col gap-1.5 rounded-2xl bg-muted/40 p-2.5">
      <div className="flex flex-wrap gap-1">
        {CHECKIN_PRESETS.map((preset) => (
          <button
            key={preset.label}
            type="button"
            onClick={() => {
              setText(preset.text);
              setSchedule(preset.schedule);
            }}
            className={cn(
              "rounded-full bg-background/70 px-2.5 py-0.5 text-[11px] text-muted-foreground hover:text-foreground",
              text === preset.text && "bg-brand/10 text-foreground shadow-[0_0_0_1px_var(--brand)]",
            )}
          >
            {preset.label}
          </button>
        ))}
      </div>
      <input
        value={text}
        onChange={(event) => setText(event.target.value)}
        placeholder={`What should ${mate.name} look at?`}
        aria-label="What the check-in is about"
        className="rounded-xl bg-background/70 px-2.5 py-1.5 text-xs text-foreground outline-none placeholder:text-muted-foreground"
      />
      <CheckinEditor
        key={JSON.stringify(schedule)}
        initial={schedule}
        onCancel={() => {
          setOpen(false);
          setError(null);
        }}
        onSave={(chosen) => {
          void matesApi
            .addCheckin(mate.id, text, chosen)
            .then(() => {
              setOpen(false);
              setText("");
              setError(null);
            })
            .catch((e) => setError(String(e).replace(/^Error:\s*/, "")));
        }}
      />
      {error && <p className="text-[11px] text-destructive">{error}</p>}
    </div>
  );
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** The schedule a check-in's `repeat` holds, for the editor; weekdays 09:00 when unreadable. */
export function scheduleOf(repeat: string | null): CheckinSchedule {
  try {
    const value = JSON.parse(repeat ?? "") as Partial<CheckinSchedule> & { kind?: string };
    const kind = value.kind === "daily" || value.kind === "weekly" ? value.kind : "weekdays";
    return { kind, time: typeof value.time === "string" ? value.time : "09:00", days: Array.isArray(value.days) ? value.days : [1] };
  } catch {
    return { kind: "weekdays", time: "09:00", days: [1] };
  }
}

/** Change a check-in's schedule (FR-70). */
function CheckinEditor({
  initial,
  onSave,
  onCancel,
}: {
  initial: CheckinSchedule;
  onSave(schedule: CheckinSchedule): void;
  onCancel(): void;
}) {
  const [schedule, setSchedule] = React.useState(initial);
  const kinds: { id: CheckinSchedule["kind"]; label: string }[] = [
    { id: "daily", label: "Every day" },
    { id: "weekdays", label: "Weekdays" },
    { id: "weekly", label: "Weekly" },
  ];
  return (
    <div className="mt-1.5 flex flex-col gap-1.5 rounded-xl bg-background/70 p-2">
      <div className="flex flex-wrap items-center gap-1">
        {kinds.map((k) => (
          <button
            key={k.id}
            type="button"
            onClick={() => setSchedule((s) => ({ ...s, kind: k.id }))}
            className={cn(
              "rounded-full px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-muted",
              schedule.kind === k.id && "bg-brand/10 text-foreground shadow-[0_0_0_1px_var(--brand)]",
            )}
          >
            {k.label}
          </button>
        ))}
        <input
          type="time"
          aria-label="Time"
          value={schedule.time}
          onChange={(event) => setSchedule((s) => ({ ...s, time: event.target.value || s.time }))}
          className="ml-auto rounded-md bg-muted/60 px-1.5 text-[11px] text-foreground outline-none"
        />
      </div>
      {schedule.kind === "weekly" && (
        <div className="flex gap-0.5">
          {DAYS.map((day, index) => {
            const on = schedule.days?.includes(index);
            return (
              <button
                key={day}
                type="button"
                onClick={() =>
                  setSchedule((s) => {
                    const days = new Set(s.days ?? []);
                    if (days.has(index)) days.delete(index);
                    else days.add(index);
                    return { ...s, days: [...days].sort() };
                  })
                }
                className={cn(
                  "flex-1 rounded-md py-0.5 text-[10px] text-muted-foreground hover:bg-muted",
                  on && "bg-brand/10 text-foreground",
                )}
              >
                {day}
              </button>
            );
          })}
        </div>
      )}
      <div className="flex justify-end gap-1">
        <Button size="xs" variant="ghost" className="h-6 rounded-full px-2 text-[11px]" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          size="xs"
          className="h-6 rounded-full bg-brand px-2.5 text-[11px] text-brand-foreground hover:bg-brand/90"
          disabled={schedule.kind === "weekly" && !schedule.days?.length}
          onClick={() => onSave(schedule)}
        >
          Save
        </Button>
      </div>
    </div>
  );
}

export function FollowingUp({ mate }: { mate: Mate }) {
  const update = useMatesStore((s) => s.update);
  const mates = useMatesStore((s) => s.mates);
  const [items, setItems] = React.useState<AgendaItem[]>([]);
  const [repeats, setRepeats] = React.useState<Record<string, string>>({});
  const [editing, setEditing] = React.useState<string | null>(null);

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
            Nothing yet. When you mention something coming up, or add a regular check-in, it shows here.
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
                        <Button
                          size="xs"
                          variant="ghost"
                          className="h-6 rounded-full px-2 text-[11px]"
                          onClick={() => setEditing(editing === item.id ? null : item.id)}
                        >
                          Change
                        </Button>
                      )}
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
                    {editing === item.id && (
                      <CheckinEditor
                        initial={scheduleOf(item.repeat)}
                        onCancel={() => setEditing(null)}
                        onSave={(schedule) => {
                          setEditing(null);
                          void matesApi
                            .rescheduleAgenda(mate.id, item.id, schedule)
                            .then((changed) =>
                              setItems((current) => current.map((i) => (i.id === changed.id ? changed : i))),
                            )
                            .catch(() => undefined);
                        }}
                      />
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        <AddCheckin mate={mate} />
      </section>
    </div>
  );
}
