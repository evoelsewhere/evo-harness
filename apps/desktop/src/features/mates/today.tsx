// Evo Mates' home, Today (docs/features/evo-mates/spec.md, FR-73; ux.md 5),
// redesigned by Evo Mates UI v2 (docs/features/evo-mates-ui-v2/spec.md,
// FR-25, FR-26): a greeting, four counters that jump to their section, the
// team at a glance (each mate's face with its presence and what it does
// now), then Needs you (oldest first, with what each waits on), New, Coming
// up and the team's day. Built from Evo's own data; it makes no model call.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { BellRingIcon, CalendarClockIcon, CheckIcon, CircleDollarSignIcon, HashIcon, InboxIcon, MessageCircleQuestionIcon, SparklesIcon, UserPlusIcon } from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { Button } from "@/components/ui/button";
import { formatCost, formatRelative } from "@/lib/format";
import { matesApi, type AgendaItem, type Mate, type Mention } from "@/lib/mates";
import { inTauri } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useMatesStore } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { EmptyState } from "./empty-state";
import { dueWords } from "./following-up";
import { plainPreview } from "./plain-preview";
import { MatePresenceFace } from "./presence-face";
import { useMateLine } from "./roster";
import { greeting, todaySections, waitingKind, type MateDay, type Place } from "./today-sections";
import { PlacedCardRow, useAssignedCards } from "./assigned-cards";

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

/** Lines that mentioned the person, newest first (evo-mates-messaging FR-18). */
function useMentions(): Mention[] {
  const [list, setList] = React.useState<Mention[]>([]);
  React.useEffect(() => {
    let gone = false;
    const read = () =>
      void matesApi
        .mentions()
        .then((all) => !gone && setList([...all].reverse()))
        .catch(() => undefined);
    read();
    const stop = inTauri ? listen("mates://mentions", read) : null;
    return () => {
      gone = true;
      void stop?.then((off) => off());
    };
  }, []);
  return list;
}

/** What each waiting chat waits on, and since when (FR-26). */
function useWaiting(chats: string[]) {
  const [found, setFound] = React.useState<Record<string, { kind: string; at: number }>>({});
  const key = chats.join("|");
  React.useEffect(() => {
    let gone = false;
    void Promise.all(
      chats.map((chat) =>
        matesApi
          .questions(chat)
          .then((list) => [chat, { kind: waitingKind(list ?? []), at: Math.min(...(list ?? []).map((q) => q.at), Date.now()) }] as const)
          .catch(() => [chat, { kind: "Waiting for you", at: Date.now() }] as const),
      ),
    ).then((pairs) => !gone && setFound(Object.fromEntries(pairs)));
    return () => {
      gone = true;
    };
    // `key` stands for the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return found;
}

function PlaceFace({ place, mates, size = 32 }: { place: Place; mates: Mate[]; size?: number }) {
  const mate = place.kind === "mate" ? mates.find((m) => m.id === place.id) : undefined;
  if (mate) return <MatePresenceFace mate={mate} size={size} ring="var(--card)" />;
  if (place.avatar) return <MateFace avatar={place.avatar} size={size} plate={false} label={place.name} />;
  return (
    <span className="flex shrink-0 items-center justify-center rounded-[10px] bg-foreground/[0.06] text-muted-foreground dark:bg-foreground/10" style={{ width: size, height: size }}>
      <HashIcon className="size-4" />
    </span>
  );
}

/** The person's open cards across boards (evo-mates-threads-board FR-14). */
function MyCards({ now }: { now: number }) {
  const cards = useAssignedCards("person");
  if (cards.length === 0) return null;
  return (
    <Section id="today-cards" title="My cards" count={cards.length} empty="">
      {cards.map((placed) => (
        <li key={placed.card.id} className="border-b border-border/50 last:border-b-0">
          <PlacedCardRow placed={placed} now={now} />
        </li>
      ))}
    </Section>
  );
}

function Section({ id, title, count, children, empty, action }: { id: string; title: string; count?: number; children: React.ReactNode; empty: string; action?: React.ReactNode }) {
  const has = React.Children.toArray(children).length > 0;
  return (
    <section id={id} className="flex scroll-mt-4 flex-col gap-2">
      <h2 className="flex items-center gap-2 px-1 text-[13px] font-semibold text-foreground">
        {title}
        {count ? <span className="rounded-full bg-muted px-1.5 text-[11px] font-medium text-muted-foreground">{count}</span> : null}
        <span className="ml-auto font-normal">{action}</span>
      </h2>
      {has ? (
        <ul className="flex flex-col overflow-hidden rounded-2xl bg-card shadow-[0_0_0_1px_var(--border)]">{children}</ul>
      ) : (
        <p className="rounded-2xl bg-muted/40 px-3.5 py-3 text-[13px] text-muted-foreground">{empty}</p>
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

/** One counter (FR-25): a number, a word, and the section it jumps to. */
function Counter({ value, label, target, icon, tone }: { value: string | number; label: string; target: string; icon: React.ReactNode; tone?: "amber" | "brand" }) {
  return (
    <button
      type="button"
      onClick={() => document.getElementById(target)?.scrollIntoView({ block: "start", behavior: "smooth" })}
      className="flex min-w-0 flex-col gap-1 rounded-2xl bg-card px-3.5 py-3 text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/40"
    >
      <span className="flex items-center gap-1.5 text-xs text-muted-foreground [&_svg]:size-3.5">
        {icon}
        {label}
      </span>
      <span
        className={cn(
          "text-[26px] leading-8 font-semibold tracking-tight text-foreground tabular-nums",
          tone === "amber" && Number(value) > 0 && "text-amber-600 dark:text-amber-400",
          tone === "brand" && Number(value) > 0 && "text-brand",
        )}
      >
        {value}
      </span>
    </button>
  );
}

/** A mate in the team strip (FR-25): face with presence, name, what it does now. */
function TeamCard({ mate }: { mate: Mate }) {
  const { presence, line } = useMateLine(mate);
  const now = presence === "idle" ? "Free" : line;
  return (
    <button
      type="button"
      onClick={() => void useMatesStore.getState().open(mate.id)}
      className="flex min-w-0 items-center gap-2.5 rounded-2xl bg-card px-3 py-2.5 text-left shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/40"
    >
      <MatePresenceFace mate={mate} size={36} live={presence === "working"} ring="var(--card)" />
      <span className="min-w-0">
        <span className="block truncate text-[13px] font-medium text-foreground">{mate.name}</span>
        <span
          className={cn(
            "block truncate text-xs",
            presence === "working" ? "text-brand" : presence === "waiting" ? "font-medium text-foreground" : "text-muted-foreground",
          )}
        >
          {now}
        </span>
      </span>
    </button>
  );
}

export function TodayPage() {
  const mates = useMatesStore((s) => s.mates);
  const channels = useMatesStore((s) => s.channels);
  const rows = useSessionStore((s) => s.sessions);
  const work = useSessionStore((s) => s.work);
  const { coming, team } = useTodayData();
  const store = useMatesStore.getState;

  const places: Place[] = React.useMemo(() => {
    const seatsOf = (mateId: string) =>
      channels.flatMap((c) => c.members.filter((m) => m.mateId === mateId && m.seatChatId).map((m) => m.seatChatId as string));
    return [
      ...mates.map((m) => ({ kind: "mate" as const, id: m.id, name: m.name, chatId: m.chatId, hidden: m.hidden, avatar: m.avatar, seats: seatsOf(m.id) })),
      ...channels.map((c) => ({
        kind: "channel" as const,
        id: c.id,
        name: `#${c.name}`,
        chatId: c.chatId,
        hidden: c.hidden,
        seats: c.members.map((m) => m.seatChatId).filter((x): x is string => Boolean(x)),
      })),
    ];
  }, [mates, channels]);
  // The chats that wait: a mate's own, or a channel's seat.
  const waitingChats = places.flatMap((p) => [p.chatId, ...(p.seats ?? [])]).filter((chat, i, all) => work[chat] === "waiting" && all.indexOf(chat) === i);
  const waiting = useWaiting(waitingChats);
  const mentions = useMentions();
  const waitingSince = Object.fromEntries(Object.entries(waiting).map(([chat, w]) => [chat, w.at]));
  const sections = todaySections({ places, rows, work, coming, team, waitingSince });
  const open = (place: Place) => void (place.kind === "mate" ? store().open(place.id) : store().openChannel(place.id));
  const now = Date.now();
  const date = new Date(now).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
  const shownMates = mates.filter((m) => !m.hidden);
  const kindOf = (place: Place) => {
    const chat = [place.chatId, ...(place.seats ?? [])].find((c) => work[c] === "waiting");
    return (chat && waiting[chat]?.kind) || "Waiting for you";
  };
  const sinceOf = (place: Place) => {
    const at = Math.min(...[place.chatId, ...(place.seats ?? [])].map((c) => waiting[c]?.at ?? Number.MAX_SAFE_INTEGER));
    return at < Number.MAX_SAFE_INTEGER ? formatRelative(at, now) : null;
  };

  const summary = [
    sections.needsYou.length ? `${sections.needsYou.length} ${sections.needsYou.length === 1 ? "thing needs" : "things need"} you` : null,
    sections.unreadCount ? `${sections.unreadCount} new` : null,
    sections.working.length ? `${sections.working.length} at work` : null,
  ].filter(Boolean);

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-7 px-6 py-8">
        <header className="flex flex-wrap items-end gap-x-4 gap-y-2 px-1">
          <div className="min-w-0 flex-1">
            <p className="text-[13px] text-muted-foreground">{date}</p>
            <h1 className="text-[28px] leading-9 font-semibold tracking-[-0.02em] text-foreground">{greeting()}</h1>
            <p className="mt-0.5 text-sm text-muted-foreground">{summary.length ? summary.join(" · ") : "All quiet. Your mates are free."}</p>
          </div>
          <div className="flex gap-2">
            <Button size="sm" variant="secondary" className="rounded-full" onClick={() => useMatesUiStore.getState().setDialog("new-channel")} disabled={mates.length < 2}>
              <HashIcon />
              New channel
            </Button>
            <Button size="sm" className="rounded-full bg-brand text-brand-foreground hover:bg-brand/90" onClick={() => store().showGallery()}>
              <UserPlusIcon />
              New mate
            </Button>
          </div>
        </header>

        {/* Counters (FR-25). */}
        <div className="grid grid-cols-2 gap-2.5 min-[720px]:grid-cols-4">
          <Counter value={sections.needsYou.length} label="Needs you" target="today-needs" icon={<InboxIcon />} tone="amber" />
          <Counter value={sections.unreadCount} label="Unread" target="today-new" icon={<SparklesIcon />} tone="brand" />
          <Counter value={sections.working.length} label="Working now" target="today-team" icon={<span className="size-2 rounded-full bg-brand" />} />
          <Counter value={formatCost(sections.totalCost)} label="Spent today" target="today-day" icon={<CircleDollarSignIcon />} />
        </div>

        {/* The team at a glance (FR-25). */}
        <section id="today-team" className="flex scroll-mt-4 flex-col gap-2">
          <h2 className="px-1 text-[13px] font-semibold text-foreground">Your team</h2>
          {shownMates.length === 0 ? (
            <EmptyState text="No mates yet. Pick a role to meet your first one." action="New mate" onAction={() => store().showGallery()} />
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-2">
              {shownMates.map((mate) => (
                <TeamCard key={mate.id} mate={mate} />
              ))}
            </div>
          )}
        </section>

        <Section id="today-needs" title="Needs you" count={sections.needsYou.length} empty="Nothing is waiting for you.">
          {sections.needsYou.map((place) => {
            const since = sinceOf(place);
            return (
              <Row key={place.chatId} label={`Open ${place.name}`} onClick={() => open(place)}>
                <PlaceFace place={place} mates={mates} />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-[13px] font-medium text-foreground">{place.name}</span>
                  {since && <span className="truncate text-xs text-muted-foreground">Waiting since {since}</span>}
                </span>
                <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11.5px] font-medium text-amber-700 dark:text-amber-300">{kindOf(place)}</span>
              </Row>
            );
          })}
        </Section>

        <MyCards now={now} />

        {mentions.length > 0 && (
          <Section id="today-mentions" title="Mentions" count={mentions.length} empty="">
            {mentions.map((mention) => {
              const channel = channels.find((c) => c.chatId === mention.chat);
              const mate = mates.find((m) => m.id === mention.author);
              return (
                <Row
                  key={`${mention.chat}:${mention.line}`}
                  label={`Open ${channel ? `#${channel.name}` : "the channel"}`}
                  onClick={() => channel && void store().openChannel(channel.id)}
                >
                  {mate ? <MatePresenceFace mate={mate} size={32} ring="var(--card)" /> : <span className="size-8" />}
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-[13px] font-medium text-foreground">
                      {mention.name} <span className="font-normal text-muted-foreground">in #{channel?.name ?? "a channel"}</span>
                    </span>
                    <span className="truncate text-xs text-muted-foreground">{mention.excerpt}</span>
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground">{formatRelative(mention.at, now)}</span>
                </Row>
              );
            })}
          </Section>
        )}

        <Section id="today-new" title="New" count={sections.fresh.length} empty="You're all caught up.">
          {sections.fresh.map(({ place, line, at }) => (
            <Row key={place.chatId} label={`Open ${place.name}`} onClick={() => open(place)}>
              <PlaceFace place={place} mates={mates} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[13px] font-semibold text-foreground">{place.name}</span>
                {line && <span className="truncate text-xs text-muted-foreground">{plainPreview(line)}</span>}
              </span>
              <span className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
                {formatRelative(at, now)}
                <span className="size-2 rounded-full bg-brand" />
              </span>
            </Row>
          ))}
        </Section>

        <Section id="today-coming" title="Coming up" count={sections.coming.length} empty="Nothing planned for today or tomorrow.">
          {sections.coming.map(({ item, mate }) => {
            const Icon = KIND_ICON[item.kind];
            return (
              <Row key={item.id} label={`${mate.name}: ${item.text}`} onClick={() => void store().open(mate.id).then(() => store().openPanel("settings"))}>
                <PlaceFace place={mate} mates={mates} />
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

        <Section
          id="today-day"
          title="Your team's day"
          empty="No mate has worked yet today."
          action={sections.team.length > 1 ? <span className="text-xs text-muted-foreground">Together {formatCost(sections.totalCost)}</span> : undefined}
        >
          {sections.team.map(({ mate, replies, costUsd }) => {
            const share = sections.totalCost > 0 ? Math.max(4, Math.round((costUsd / sections.totalCost) * 100)) : 0;
            return (
              <Row key={mate.id} label={`Open ${mate.name}`} onClick={() => open(mate)}>
                <PlaceFace place={mate} mates={mates} />
                <span className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="truncate text-[13px] font-medium text-foreground">{mate.name}</span>
                  <span className="h-1 w-full max-w-48 overflow-hidden rounded-full bg-muted">
                    <span className="block h-full rounded-full bg-brand" style={{ width: `${share}%` }} />
                  </span>
                </span>
                <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                  {replies} {replies === 1 ? "reply" : "replies"} · {formatCost(costUsd)}
                </span>
              </Row>
            );
          })}
        </Section>
      </div>
    </div>
  );
}
