// A channel's board (docs/features/evo-mates-threads-board/spec.md, FR-8 –
// FR-10): columns of cards the person drags within and between columns,
// quick add at a column's top, filters by assignee, label and priority, a
// search, a list view, and columns to rename, add, move or remove. A card
// opens in the card sheet (`card-sheet.tsx`).

import * as React from "react";
import {
  CalendarIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  Columns3Icon,
  ListIcon,
  ListChecksIcon,
  MessagesSquareIcon,
  MoreHorizontalIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  SignalHighIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { MateFace } from "@/components/mate-face/mate-face";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { boardsApi, type Card, type Channel, type Mate } from "@/lib/mates";
import { cn } from "@/lib/utils";
import { useBoardStore } from "@/stores/board-store";
import { cardsIn, dropIndex, dueLabel, filterCards, filtering, labelsOf, NO_FILTER, overdue, PRIORITIES, type CardFilter } from "./board-logic";
import { useMembers } from "./channel-header";

/** A card's assignee as a face: a mate's, or "You" for the person. */
export function AssigneeFace({ id, members, size = 20 }: { id: string | null; members: Mate[]; size?: number }) {
  if (!id) return null;
  if (id === "person") {
    return (
      <span
        title="You"
        style={{ width: size, height: size }}
        className="flex shrink-0 items-center justify-center rounded-full bg-brand text-[9px] font-semibold text-brand-foreground"
      >
        You
      </span>
    );
  }
  const mate = members.find((m) => m.id === id);
  if (!mate) return <span title={id} style={{ width: size, height: size }} className="shrink-0 rounded-full bg-muted opacity-60" />;
  return <MateFace avatar={mate.avatar} size={size} plate={false} label={mate.name} />;
}

/** A priority's mark (FR-9). */
export function PriorityMark({ priority }: { priority: Card["priority"] }) {
  if (priority === "none") return null;
  const meta = PRIORITIES.find((p) => p.id === priority)!;
  return (
    <span className={cn("flex items-center gap-0.5 text-[11px] font-medium", meta.tone)} title={`${meta.label} priority`}>
      <SignalHighIcon className="size-3" />
      {meta.label}
    </span>
  );
}

function CardTile({
  card,
  members,
  lastColumn,
  onOpen,
  onDragStart,
  dragging,
}: {
  card: Card;
  members: Mate[];
  lastColumn: string | undefined;
  onOpen(): void;
  onDragStart(event: React.DragEvent): void;
  dragging: boolean;
}) {
  const late = overdue(card, lastColumn);
  const done = card.checklist.filter((c) => c.done).length;
  return (
    <button
      type="button"
      draggable
      onDragStart={onDragStart}
      onClick={onOpen}
      data-card={card.key}
      className={cn(
        "group/card flex w-full flex-col gap-1.5 rounded-xl bg-background p-2.5 text-left shadow-[0_0_0_1px_var(--border),0_1px_2px_rgb(0_0_0/0.04)] transition-shadow hover:shadow-[0_0_0_1px_var(--border),0_4px_12px_-4px_rgb(0_0_0/0.15)]",
        dragging && "opacity-40",
      )}
    >
      <span className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <span className="font-mono tabular-nums">{card.key}</span>
        {card.threadRoot && <MessagesSquareIcon className="size-3" aria-label="Has a thread" />}
        <span className="flex-1" />
        <AssigneeFace id={card.assignee} members={members} size={18} />
      </span>
      <span className="line-clamp-3 text-[13px] leading-[18px] text-foreground [overflow-wrap:anywhere]">{card.title}</span>
      {(card.priority !== "none" || card.dueMs !== null || card.labels.length > 0 || card.checklist.length > 0) && (
        <span className="flex flex-wrap items-center gap-1.5">
          <PriorityMark priority={card.priority} />
          {card.dueMs !== null && (
            <span className={cn("flex items-center gap-0.5 rounded-full px-1.5 py-px text-[11px]", late ? "bg-destructive/10 text-destructive" : "bg-muted text-muted-foreground")}>
              <CalendarIcon className="size-3" />
              {dueLabel(card.dueMs)}
            </span>
          )}
          {card.checklist.length > 0 && (
            <span className="flex items-center gap-0.5 text-[11px] text-muted-foreground">
              <ListChecksIcon className="size-3" />
              {done}/{card.checklist.length}
            </span>
          )}
          {card.labels.map((label) => (
            <span key={label} className="max-w-28 truncate rounded-full bg-brand/10 px-1.5 py-px text-[11px] text-brand">
              {label}
            </span>
          ))}
        </span>
      )}
    </button>
  );
}

function QuickAdd({ onAdd, onDone }: { onAdd(title: string): void; onDone(): void }) {
  const [title, setTitle] = React.useState("");
  return (
    <input
      autoFocus
      value={title}
      placeholder="Card title, Enter to add"
      onChange={(e) => setTitle(e.target.value)}
      onBlur={() => !title.trim() && onDone()}
      onKeyDown={(e) => {
        if (e.key === "Escape") onDone();
        if (e.key === "Enter" && title.trim()) {
          onAdd(title.trim());
          setTitle("");
        }
      }}
      className="w-full rounded-xl border border-brand/50 bg-background px-2.5 py-2 text-[13px] outline-none ring-2 ring-brand/15"
    />
  );
}

function FilterSelect({ label, value, onChange, options }: { label: string; value: string; onChange(v: string): void; options: { value: string; label: string }[] }) {
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className={cn(
        "h-8 max-w-36 rounded-full border border-border bg-background px-2.5 text-[12.5px] outline-none focus-visible:ring-2 focus-visible:ring-brand/20",
        value ? "text-foreground" : "text-muted-foreground",
      )}
    >
      <option value="">{label}</option>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function BoardView({ channel }: { channel: Channel }) {
  const members = useMembers(channel);
  const view = useBoardStore((s) => s.views[channel.id]);
  const [filter, setFilter] = React.useState<CardFilter>(NO_FILTER);
  const [mode, setMode] = React.useState<"columns" | "list">("columns");
  const [adding, setAdding] = React.useState<string | null>(null);
  const [newColumn, setNewColumn] = React.useState<string | null>(null);
  const [renaming, setRenaming] = React.useState<string | null>(null);
  const [drag, setDrag] = React.useState<{ id: string; column: string; before: string | null } | null>(null);
  React.useEffect(() => {
    void useBoardStore.getState().load(channel.id);
  }, [channel.id]);

  const board = view?.board;
  if (!view) return <div className="flex h-full items-center justify-center text-sm text-muted-foreground">Opening the board…</div>;
  if (!board?.enabled) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <Columns3Icon className="size-8 text-muted-foreground" />
        <p className="max-w-sm text-sm text-muted-foreground">This channel's board is off.</p>
        <button type="button" onClick={() => void useBoardStore.getState().turn(channel.id, true, view.proposedKey)} className="rounded-full bg-brand px-4 py-1.5 text-sm font-medium text-brand-foreground">
          Turn on board
        </button>
      </div>
    );
  }
  const store = useBoardStore.getState();
  const shown = filterCards(view.cards, filter);
  const last = board.columns.at(-1)?.id;
  const open = (card: Card) => store.showCard(card.key);
  const columnAction = (action: "rename" | "move" | "remove", column: string, args: { name?: string; index?: number } = {}) =>
    void boardsApi
      .column(channel.id, action, { column, ...args })
      .then(() => store.load(channel.id))
      .catch(() => undefined);
  const drop = (column: string) => {
    if (!drag) return;
    const index = dropIndex(cardsIn(view.cards, column), drag.id, drag.before);
    setDrag(null);
    void store.move(channel.id, drag.id, column, index).catch(() => undefined);
  };
  const labels = labelsOf(view.cards);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Filters and search (FR-10). */}
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 px-4 py-2.5">
        <label className="flex h-8 min-w-0 flex-1 basis-40 items-center gap-1.5 rounded-full border border-border bg-background px-3 focus-within:ring-2 focus-within:ring-brand/20">
          <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
          <input
            value={filter.query}
            onChange={(e) => setFilter({ ...filter, query: e.target.value })}
            placeholder={`Search ${board.key} cards`}
            aria-label="Search cards"
            className="min-w-0 flex-1 bg-transparent text-[12.5px] outline-none placeholder:text-muted-foreground"
          />
        </label>
        <FilterSelect
          label="Assignee"
          value={filter.assignee}
          onChange={(assignee) => setFilter({ ...filter, assignee })}
          options={[{ value: "person", label: "You" }, ...members.map((m) => ({ value: m.id, label: m.name })), { value: "none", label: "Nobody" }]}
        />
        {labels.length > 0 && <FilterSelect label="Label" value={filter.label} onChange={(label) => setFilter({ ...filter, label })} options={labels.map((l) => ({ value: l, label: l }))} />}
        <FilterSelect
          label="Priority"
          value={filter.priority}
          onChange={(priority) => setFilter({ ...filter, priority: priority as CardFilter["priority"] })}
          options={PRIORITIES.filter((p) => p.id !== "none").map((p) => ({ value: p.id, label: p.label }))}
        />
        {filtering(filter) && (
          <button type="button" onClick={() => setFilter(NO_FILTER)} className="flex h-8 items-center gap-1 rounded-full px-2.5 text-[12.5px] text-muted-foreground hover:bg-muted">
            <XIcon className="size-3.5" />
            Clear
          </button>
        )}
        <div className="ml-auto flex h-8 items-center rounded-full bg-muted p-0.5" role="group" aria-label="View">
          {(["columns", "list"] as const).map((m) => (
            <button
              key={m}
              type="button"
              aria-pressed={mode === m}
              onClick={() => setMode(m)}
              className={cn("flex h-7 items-center gap-1 rounded-full px-2.5 text-[12px]", mode === m ? "bg-background text-foreground shadow-sm" : "text-muted-foreground")}
            >
              {m === "columns" ? <Columns3Icon className="size-3.5" /> : <ListIcon className="size-3.5" />}
              {m === "columns" ? "Columns" : "List"}
            </button>
          ))}
        </div>
      </div>

      {mode === "list" ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
          {shown.length === 0 ? (
            <p className="py-10 text-center text-sm text-muted-foreground">{filtering(filter) ? "No card matches." : "No cards yet."}</p>
          ) : (
            <ul className="divide-y divide-border/60">
              {board.columns.flatMap((column) =>
                cardsIn(shown, column.id).map((card) => (
                  <li key={card.id}>
                    <button type="button" onClick={() => open(card)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-muted/60">
                      <span className="w-20 shrink-0 font-mono text-[11.5px] text-muted-foreground tabular-nums">{card.key}</span>
                      <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{card.title}</span>
                      <span className="hidden w-24 shrink-0 truncate text-[12px] text-muted-foreground sm:block">{column.name}</span>
                      <span className="hidden w-20 shrink-0 md:block">
                        <PriorityMark priority={card.priority} />
                      </span>
                      <span className={cn("w-20 shrink-0 text-right text-[12px]", overdue(card, last) ? "text-destructive" : "text-muted-foreground")}>
                        {card.dueMs !== null ? dueLabel(card.dueMs) : ""}
                      </span>
                      <span className="flex w-6 shrink-0 justify-end">
                        <AssigneeFace id={card.assignee} members={members} size={20} />
                      </span>
                    </button>
                  </li>
                )),
              )}
            </ul>
          )}
        </div>
      ) : (
        <div data-scrolls-x className="flex min-h-0 flex-1 gap-3 overflow-x-auto overflow-y-hidden p-4">
          {board.columns.map((column, index) => {
            const cards = cardsIn(shown, column.id);
            const dropping = drag?.column === column.id;
            return (
              <section
                key={column.id}
                aria-label={column.name}
                onDragOver={(e) => {
                  if (!drag) return;
                  e.preventDefault();
                  if (drag.column !== column.id || drag.before !== null) {
                    // Over the column's empty space: the end.
                    if (e.target === e.currentTarget) setDrag({ ...drag, column: column.id, before: null });
                  }
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  drop(column.id);
                }}
                className={cn("flex h-full w-[272px] shrink-0 flex-col rounded-2xl bg-muted/50 transition-colors", dropping && "bg-brand/[0.06]")}
              >
                <header className="flex shrink-0 items-center gap-1.5 px-3 pt-2.5 pb-1.5">
                  {renaming === column.id ? (
                    <input
                      autoFocus
                      defaultValue={column.name}
                      aria-label="Column name"
                      onBlur={(e) => {
                        setRenaming(null);
                        if (e.target.value.trim() && e.target.value.trim() !== column.name) columnAction("rename", column.id, { name: e.target.value.trim() });
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") e.currentTarget.blur();
                        if (e.key === "Escape") setRenaming(null);
                      }}
                      className="h-7 min-w-0 flex-1 rounded-lg border border-border bg-background px-2 text-[13px] font-medium outline-none"
                    />
                  ) : (
                    <button type="button" onDoubleClick={() => setRenaming(column.id)} className="min-w-0 truncate text-left text-[13px] font-semibold text-foreground">
                      {column.name}
                    </button>
                  )}
                  <span className="text-[12px] text-muted-foreground tabular-nums">{cards.length}</span>
                  <span className="flex-1" />
                  <button type="button" aria-label={`Add a card to ${column.name}`} onClick={() => setAdding(column.id)} className="flex size-6 items-center justify-center rounded-full text-muted-foreground hover:bg-background hover:text-foreground">
                    <PlusIcon className="size-3.5" />
                  </button>
                  <DropdownMenu>
                    <DropdownMenuTrigger
                      render={<button type="button" aria-label={`${column.name}: more`} className="flex size-6 items-center justify-center rounded-full text-muted-foreground hover:bg-background hover:text-foreground" />}
                    >
                      <MoreHorizontalIcon className="size-3.5" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-44">
                      <DropdownMenuItem onClick={() => setRenaming(column.id)}>
                        <PencilIcon />
                        Rename
                      </DropdownMenuItem>
                      <DropdownMenuItem disabled={index === 0} onClick={() => columnAction("move", column.id, { index: index - 1 })}>
                        <ChevronLeftIcon />
                        Move left
                      </DropdownMenuItem>
                      <DropdownMenuItem disabled={index === board.columns.length - 1} onClick={() => columnAction("move", column.id, { index: index + 1 })}>
                        <ChevronRightIcon />
                        Move right
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem variant="destructive" disabled={board.columns.length === 1} onClick={() => columnAction("remove", column.id)}>
                        <Trash2Icon />
                        Remove column
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </header>
                <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2 pt-1 pb-3">
                  {adding === column.id && (
                    <QuickAdd
                      onDone={() => setAdding(null)}
                      onAdd={(title) => void store.create(channel.id, { title, columnId: column.id, top: true }).catch(() => undefined)}
                    />
                  )}
                  {cards.map((card) => (
                    <div
                      key={card.id}
                      onDragOver={(e) => {
                        if (!drag) return;
                        e.preventDefault();
                        if (drag.column !== column.id || drag.before !== card.id) setDrag({ ...drag, column: column.id, before: card.id });
                      }}
                      className="relative"
                    >
                      {dropping && drag?.before === card.id && drag.id !== card.id && <span aria-hidden className="absolute -top-1.5 right-1 left-1 h-0.5 rounded-full bg-brand" />}
                      <CardTile
                        card={card}
                        members={members}
                        lastColumn={last}
                        dragging={drag?.id === card.id}
                        onOpen={() => open(card)}
                        onDragStart={(e) => {
                          e.dataTransfer.effectAllowed = "move";
                          e.dataTransfer.setData("text/plain", card.key);
                          setDrag({ id: card.id, column: column.id, before: card.id });
                        }}
                      />
                    </div>
                  ))}
                  {dropping && drag?.before === null && <span aria-hidden className="mx-1 h-0.5 shrink-0 rounded-full bg-brand" />}
                  {cards.length === 0 && adding !== column.id && (
                    <p onDragOver={(e) => drag && e.preventDefault()} className="px-2 py-6 text-center text-[12px] text-muted-foreground">
                      {filtering(filter) ? "No card matches." : "No cards"}
                    </p>
                  )}
                </div>
              </section>
            );
          })}
          <div className="w-[220px] shrink-0">
            {newColumn !== null ? (
              <input
                autoFocus
                value={newColumn}
                placeholder="Column name"
                aria-label="New column name"
                onChange={(e) => setNewColumn(e.target.value)}
                onBlur={() => setNewColumn(null)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setNewColumn(null);
                  if (e.key === "Enter" && newColumn.trim()) {
                    const name = newColumn.trim();
                    setNewColumn(null);
                    void boardsApi
                      .column(channel.id, "add", { name })
                      .then(() => store.load(channel.id))
                      .catch(() => undefined);
                  }
                }}
                className="h-9 w-full rounded-xl border border-border bg-background px-3 text-[13px] outline-none"
              />
            ) : (
              <button type="button" onClick={() => setNewColumn("")} className="flex h-9 w-full items-center gap-1.5 rounded-xl px-3 text-[13px] text-muted-foreground hover:bg-muted">
                <PlusIcon className="size-3.5" />
                Add column
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
