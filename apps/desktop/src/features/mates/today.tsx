// Evo Mates' home, Today (docs/features/evo-mates/spec.md, FR-73; ux.md 5):
// what needs the person, what is new, what is coming up and what each mate
// did today. Built from Evo's own data; it makes no model call.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { BellRingIcon, CalendarClockIcon, CheckIcon, HashIcon, MessageCircleQuestionIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { formatCost, formatRelative } from "@/lib/format";
import { matesApi, type AgendaItem } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { useMatesStore } from "@/stores/mates-store";
import { useSessionStore } from "@/stores/session-store";
import { dueWords } from "./following-up";
import { todaySections, type MateDay, type Place } from "./today-sections";

const KIND_ICON = { promise: CheckIcon, event: CalendarClockIcon, question: MessageCircleQuestionIcon, checkin: BellRingIcon } as const;

/** Coming up and the team's day from the shell, read again when mates or work change. */
function useTodayData() {
  const [data, setData] = React.useState<{ coming: AgendaItem[]; team: MateDay[] }>({ coming: [], team: [] });
  const work = useSessionStore((s) => s.work);
  const busy = Object.values(work).filter((w) => w === "working").length;
  React.useEffect(() => {
    let gone = false;
    const read = () =>
      void matesApi
        .today()
        .then((next) => !gone && setData(next))
        .catch(() => undefined);
    read();
    const stop = inTauri ? listen("mates://changed", read) : null;
    return () => {
      gone = true;
      void stop?.then((off) => off());
    };
  }, [busy]);
  return data;
}

function PlaceFace({ place, size = 28 }: { place: Place; size?: number }) {
  if (place.kind === "channel" || !place.avatar)
    return (
      <span className="flex shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground" style={{ width: size, height: size }}>
        <HashIcon className="size-4" />
      </span>
    );
  return <MateFace avatar={place.avatar} size={size} plate={false} label={place.name} />;
}

function Section({ title, count, children, empty }: { title: string; count?: number; children: React.ReactNode; empty: string }) {
  const has = React.Children.count(children) > 0;
  return (
    <section className="flex flex-col gap-1.5">
      <h2 className="flex items-center gap-2 px-1 text-xs font-medium text-muted-foreground">
        {title}
        {count ? <span className="rounded-full bg-muted px-1.5 text-[11px] text-foreground">{count}</span> : null}
      </h2>
      {has ? (
        <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{children}</ul>
      ) : (
        <p className="px-1 text-[13px] text-muted-foreground/80">{empty}</p>
      )}
    </section>
  );
}

function Row({ onClick, children, label }: { onClick(): void; children: React.ReactNode; label: string }) {
  return (
    <li className="border-b border-border/50 last:border-b-0">
      <button type="button" aria-label={label} onClick={onClick} className="flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-accent/60">
        {children}
      </button>
    </li>
  );
}

export function TodayPage() {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const rows = useSessionStore((s) => s.sessions);
  const work = useSessionStore((s) => s.work);
  const { coming, team } = useTodayData();
  const store = useMatesStore.getState;

  const places: Place[] = React.useMemo(
    () => [
      ...mates.map((m) => ({ kind: "mate" as const, id: m.id, name: m.name, chatId: m.chatId, hidden: m.hidden, avatar: m.avatar })),
      ...channels.map((c) => ({ kind: "channel" as const, id: c.id, name: `#${c.name}`, chatId: c.chatId, hidden: c.hidden })),
    ],
    [mates, channels],
  );
  const sections = todaySections({ places, rows, work, coming, team });
  const open = (place: Place) => void (place.kind === "mate" ? store().open(place.id) : store().openChannel(place.id));
  const now = Date.now();
  const date = new Date(now).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-2xl flex-col gap-6 px-6 py-8">
        <header className="flex flex-col gap-1 px-1">
          <h1 className="text-2xl font-semibold tracking-tight text-foreground">Today</h1>
          <p className="text-sm text-muted-foreground">{date}</p>
        </header>

        <Section title="Needs you" count={sections.needsYou.length} empty="Nothing is waiting for you.">
          {sections.needsYou.map((place) => (
            <Row key={place.chatId} label={`Open ${place.name}`} onClick={() => open(place)}>
              <PlaceFace place={place} />
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{place.name}</span>
              <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <span className="size-2 rounded-full bg-primary" />
                Needs you
              </span>
            </Row>
          ))}
        </Section>

        <Section title="New" count={sections.fresh.length} empty="You're all caught up.">
          {sections.fresh.map(({ place, line, at }) => (
            <Row key={place.chatId} label={`Open ${place.name}`} onClick={() => open(place)}>
              <PlaceFace place={place} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[13px] font-medium text-foreground">{place.name}</span>
                {line && <span className="truncate text-xs text-muted-foreground">{line}</span>}
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">{formatRelative(at, now)}</span>
            </Row>
          ))}
        </Section>

        <Section title="Coming up" count={sections.coming.length} empty="Nothing planned for today or tomorrow.">
          {sections.coming.map(({ item, mate }) => {
            const Icon = KIND_ICON[item.kind];
            return (
              <Row
                key={item.id}
                label={`${mate.name}: ${item.text}`}
                onClick={() => void store().open(mate.id).then(() => store().openPanel("settings"))}
              >
                <PlaceFace place={mate} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-[13px] text-foreground">{item.text}</span>
                  <span className="truncate text-xs text-muted-foreground">{mate.name}</span>
                </span>
                <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                  <Icon className="size-3.5" />
                  {dueWords(item.dueAt, now)}
                </span>
              </Row>
            );
          })}
        </Section>

        <Section title="Your team today" empty="No mate has worked yet today.">
          {sections.team.map(({ mate, replies, costUsd }) => (
            <Row key={mate.id} label={`Open ${mate.name}`} onClick={() => open(mate)}>
              <PlaceFace place={mate} />
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{mate.name}</span>
              <span className="text-xs text-muted-foreground">
                {replies} {replies === 1 ? "reply" : "replies"} · {formatCost(costUsd)}
              </span>
            </Row>
          ))}
        </Section>
        {sections.team.length > 1 && (
          <p className="-mt-4 px-1 text-xs text-muted-foreground">Together today: {formatCost(sections.totalCost)}</p>
        )}
      </div>
    </div>
  );
}
