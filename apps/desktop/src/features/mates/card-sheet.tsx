// A board card in a side sheet (docs/features/evo-mates-threads-board/spec.md,
// FR-9, FR-10, FR-15): every field to edit, its checklist, its links (its
// thread opens in its channel), its activity log with comments, and Delete.
// One sheet for the window: a board, a `KEY-n` link, Today and a profile all
// open it through `useBoardStore.showCard`.

import * as React from "react";
import { CheckIcon, MessagesSquareIcon, PlusIcon, Trash2Icon, XIcon } from "lucide-react";
import { Markdown } from "@/components/markdown";
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet";
import { formatRelative } from "@/lib/format";
import { boardsApi, type CardEvent, type CardPatch, type PlacedCard } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/board-store";
import { useMatesStore } from "@/stores/mates-store";
import { dateValue, dueFromDate, PRIORITIES } from "./board-logic";
import { AssigneeFace } from "./board-view";
import { ConfirmDelete } from "./confirm";

const field = "h-8 w-full rounded-lg border border-border bg-background px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-brand/20";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid grid-cols-[88px_1fr] items-center gap-2 text-[12.5px]">
      <span className="text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

export function CardSheet() {
  const key = useBoardStore((s) => s.openCard);
  const [placed, setPlaced] = React.useState<PlacedCard | null>(null);
  const [events, setEvents] = React.useState<CardEvent[]>([]);
  const [missing, setMissing] = React.useState(false);
  const [editing, setEditing] = React.useState(false);
  const [comment, setComment] = React.useState("");
  const [item, setItem] = React.useState("");
  const [label, setLabel] = React.useState("");
  const [confirm, setConfirm] = React.useState(false);
  const channels = useMatesStore((s) => s.channels);
  const mates = useMatesStore((s) => s.mates);
  const view = useBoardStore((s) => (placed ? s.views[placed.card.channelId] : undefined));
  // The title follows changes made elsewhere, but never while the person types (board review #6).
  const titleRef = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    const el = titleRef.current;
    if (el && placed && document.activeElement !== el && el.value !== placed.card.title) el.value = placed.card.title;
  }, [placed]);
  // Labels and checklist change from what the card holds now, never from this snapshot (board review #6).
  const after = (work: Promise<unknown>) =>
    void work
      .then(() => {
        void reload();
        if (placed) void useBoardStore.getState().load(placed.card.channelId);
      })
      .catch(() => undefined);

  const reload = React.useCallback(async () => {
    if (!key) return;
    const found = await boardsApi.byKey(key).catch(() => null);
    setPlaced(found);
    setMissing(!found);
    if (found) {
      setEvents(await boardsApi.events(found.card.id).catch(() => []));
      if (!useBoardStore.getState().views[found.card.channelId]) void useBoardStore.getState().load(found.card.channelId);
    }
  }, [key]);
  React.useEffect(() => {
    setPlaced(null);
    setMissing(false);
    setEditing(false);
    void reload();
  }, [reload]);
  // Another change to this card (a mate's tool) shows here too.
  const stamp = view?.cards.find((c) => c.id === placed?.card.id)?.updatedMs;
  React.useEffect(() => {
    if (stamp && placed && stamp !== placed.card.updatedMs) void reload();
  }, [stamp, placed, reload]);

  const close = () => useBoardStore.getState().showCard(null);
  const card = placed?.card;
  const channel = channels.find((c) => c.id === card?.channelId);
  const members = channel ? channel.members.map((m) => mates.find((x) => x.id === m.mateId)).filter((m): m is NonNullable<typeof m> => Boolean(m)) : [];
  const columns = view?.board?.columns ?? [];
  const nameOf = (id: string) => (id === "person" ? "You" : (mates.find((m) => m.id === id)?.name ?? id));
  const save = (patch: CardPatch) => {
    if (!card) return;
    void boardsApi
      .update(card.id, patch)
      .then(() => {
        void reload();
        void useBoardStore.getState().load(card.channelId);
      })
      .catch(() => undefined);
  };

  return (
    <Sheet open={key !== null} onOpenChange={(open) => !open && close()}>
      <SheetContent side="right" showCloseButton={false} className="w-full gap-0 p-0 sm:max-w-[460px]">
        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
          <SheetTitle className="min-w-0 flex-1 truncate text-[13px] font-medium text-muted-foreground">
            <span className="font-mono text-foreground">{key}</span>
            {placed && <span> · #{placed.channelName}</span>}
          </SheetTitle>
          <button type="button" aria-label="Close card" onClick={close} className="rounded-full p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground">
            <XIcon className="size-4" />
          </button>
        </div>
        {missing ? (
          <p className="p-6 text-sm text-muted-foreground">There is no card {key}.</p>
        ) : !card ? (
          <p className="p-6 text-sm text-muted-foreground">Opening…</p>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
            <textarea
              key={`title:${card.id}`}
              ref={titleRef}
              defaultValue={card.title}
              aria-label="Title"
              rows={1}
              onBlur={(e) => e.target.value.trim() && e.target.value.trim() !== card.title && save({ title: e.target.value.trim().replace(/\s+/g, " ") })}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  e.currentTarget.blur();
                }
              }}
              className="w-full resize-none rounded-lg bg-transparent px-1 py-1 text-[17px] leading-6 font-semibold text-foreground outline-none [field-sizing:content] hover:bg-muted/50 focus:bg-muted/50"
            />
            <div className="mt-3 flex flex-col gap-2">
              <Row label="Status">
                <select
                  value={card.columnId}
                  className={field}
                  onChange={(e) =>
                    void useBoardStore
                      .getState()
                      .move(card.channelId, card.id, e.target.value, null)
                      .then(reload)
                      .catch(() => undefined)
                  }
                >
                  {columns.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </Row>
              <Row label="Assignee">
                <span className="flex items-center gap-2">
                  <AssigneeFace id={card.assignee} members={members} size={22} />
                  <select value={card.assignee ?? ""} className={field} onChange={(e) => save({ assignee: e.target.value || null })}>
                    <option value="">Nobody</option>
                    <option value="person">You</option>
                    {members.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name}
                      </option>
                    ))}
                  </select>
                </span>
              </Row>
              <Row label="Priority">
                <select value={card.priority} className={field} onChange={(e) => save({ priority: e.target.value as typeof card.priority })}>
                  {PRIORITIES.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </Row>
              <Row label="Due">
                <input type="date" value={dateValue(card.dueMs)} className={field} onChange={(e) => save({ dueMs: dueFromDate(e.target.value) })} />
              </Row>
              <Row label="Labels">
                <span className="flex flex-wrap items-center gap-1">
                  {card.labels.map((l) => (
                    <span key={l} className="flex items-center gap-0.5 rounded-full bg-brand/10 py-0.5 pr-1 pl-2 text-[12px] text-brand">
                      {l}
                      <button type="button" aria-label={`Remove ${l}`} onClick={() => after(boardsApi.labels(card.id, [], [l]))} className="rounded-full p-0.5 hover:bg-brand/15">
                        <XIcon className="size-3" />
                      </button>
                    </span>
                  ))}
                  <input
                    value={label}
                    placeholder="Add label"
                    aria-label="Add label"
                    onChange={(e) => setLabel(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && label.trim()) {
                        after(boardsApi.labels(card.id, [label.trim()], []));
                        setLabel("");
                      }
                    }}
                    className="h-7 w-24 min-w-0 flex-1 rounded-lg bg-transparent px-1.5 text-[12.5px] outline-none placeholder:text-muted-foreground"
                  />
                </span>
              </Row>
            </div>

            <h3 className="mt-5 mb-1.5 text-[12px] font-semibold tracking-wide text-muted-foreground uppercase">Description</h3>
            {editing ? (
              <textarea
                autoFocus
                defaultValue={card.description}
                aria-label="Description"
                rows={6}
                onBlur={(e) => {
                  setEditing(false);
                  if (e.target.value.trim() !== card.description) save({ description: e.target.value });
                }}
                className="w-full rounded-lg border border-border bg-background p-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-brand/20"
              />
            ) : (
              <button type="button" onClick={() => setEditing(true)} className="block w-full rounded-lg px-1 py-1 text-left hover:bg-muted/50">
                {card.description ? <Markdown className="text-[13px]">{card.description}</Markdown> : <span className="text-[13px] text-muted-foreground">Add a description…</span>}
              </button>
            )}

            <h3 className="mt-5 mb-1.5 text-[12px] font-semibold tracking-wide text-muted-foreground uppercase">
              Checklist {card.checklist.length > 0 && `· ${card.checklist.filter((c) => c.done).length}/${card.checklist.length}`}
            </h3>
            <ul className="flex flex-col gap-0.5">
              {card.checklist.map((c, i) => (
                <li key={i} className="group/item flex items-center gap-2 rounded-lg px-1 py-0.5 hover:bg-muted/50">
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={c.done}
                    aria-label={c.text}
                    onClick={() => after(boardsApi.checklist(card.id, c.done ? { uncheck: [i + 1] } : { check: [i + 1] }))}
                    className={cn("flex size-4 shrink-0 items-center justify-center rounded border", c.done ? "border-brand bg-brand text-brand-foreground" : "border-border")}
                  >
                    {c.done && <CheckIcon className="size-3" />}
                  </button>
                  <span className={cn("min-w-0 flex-1 text-[13px]", c.done && "text-muted-foreground line-through")}>{c.text}</span>
                  <button type="button" aria-label={`Remove ${c.text}`} onClick={() => after(boardsApi.checklist(card.id, { remove: [i + 1] }))} className="hidden rounded p-0.5 text-muted-foreground group-hover/item:block hover:text-destructive">
                    <XIcon className="size-3.5" />
                  </button>
                </li>
              ))}
            </ul>
            <label className="mt-1 flex items-center gap-2 px-1">
              <PlusIcon className="size-4 text-muted-foreground" />
              <input
                value={item}
                placeholder="Add an item"
                aria-label="Add a checklist item"
                onChange={(e) => setItem(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && item.trim()) {
                    after(boardsApi.checklist(card.id, { add: [item.trim()] }));
                    setItem("");
                  }
                }}
                className="h-7 min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
              />
            </label>

            {/* Its thread, opened in its channel (FR-15). */}
            {card.threadRoot && placed && (
              <button
                type="button"
                onClick={() => {
                  useBoardStore.getState().showThread({ chat: placed.chatId, root: card.threadRoot! });
                  useBoardStore.getState().setTab(placed.chatId, "chat");
                  close();
                  void useMatesStore.getState().openChat(placed.chatId);
                }}
                className="mt-5 flex w-full items-center gap-2 rounded-xl bg-muted/60 px-3 py-2 text-left text-[13px] text-foreground hover:bg-muted"
              >
                <MessagesSquareIcon className="size-4 text-muted-foreground" />
                Open its thread in #{placed.channelName}
              </button>
            )}

            <h3 className="mt-5 mb-1.5 text-[12px] font-semibold tracking-wide text-muted-foreground uppercase">Activity</h3>
            <ul className="flex flex-col gap-2">
              {events.map((event) => (
                <li key={event.id} className="flex items-start gap-2 text-[12.5px]">
                  <AssigneeFace id={event.actor} members={members} size={18} />
                  <span className={cn("min-w-0 flex-1", event.kind === "comment" ? "rounded-xl bg-muted/60 px-2.5 py-1.5 text-foreground" : "pt-px text-muted-foreground")}>
                    <span className="font-medium text-foreground">{nameOf(event.actor)}</span> {event.kind === "comment" ? <span className="block whitespace-pre-wrap">{event.text}</span> : event.text}
                    <span className="ml-1 text-[11px] text-muted-foreground">{formatRelative(event.atMs)}</span>
                  </span>
                </li>
              ))}
            </ul>
            <textarea
              value={comment}
              rows={2}
              placeholder="Write a comment, Enter to post"
              aria-label="Comment"
              onChange={(e) => setComment(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey && comment.trim()) {
                  e.preventDefault();
                  const text = comment.trim();
                  setComment("");
                  void boardsApi.comment(card.id, text).then(reload).catch(() => undefined);
                }
              }}
              className="mt-2 w-full rounded-xl border border-border bg-background p-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-brand/20"
            />
            <button type="button" onClick={() => setConfirm(true)} className="mt-6 mb-2 flex items-center gap-1.5 rounded-full px-2 py-1 text-[12.5px] text-destructive hover:bg-destructive/10">
              <Trash2Icon className="size-3.5" />
              Delete card
            </button>
            <ConfirmDelete
              open={confirm}
              onOpenChange={setConfirm}
              title={`Delete ${card.key}?`}
              description="The card and its activity go. Its number is not used again."
              onConfirm={() => {
                void boardsApi
                  .remove(card.id)
                  .then(() => useBoardStore.getState().load(card.channelId))
                  .catch(() => undefined);
                close();
              }}
            />
          </div>
        )}
      </SheetContent>
    </Sheet>
  );
}
