import * as React from "react";
import { ChatView } from "@/features/chat/chat-view";
import { cn } from "@/lib/utils";
import { PaneChatContext, useEngineStore } from "@/stores/engine-store";
import { DIVIDER_WIDTH, MIN_PANE_HEIGHT, MIN_PANE_WIDTH, arrange, clampRatio, type Arrangement, type PaneIndex } from "@/stores/split-rules";
import { useSplitStore } from "@/stores/split-store";
import { registerDropArea } from "./drag";
import { DropPreview } from "./drop-overlay";
import { enter, flip, leave } from "./motion";
import { PaneHeader } from "./pane-header";

type Slot = "a" | "b" | "c" | "d";
const SLOTS: Slot[] = ["a", "b", "c", "d"];

/**
 * Stable React keys for the panes, so a pane's chat view stays mounted (its
 * scroll, its composer) while its place changes: a chat keeps its slot, and a
 * chat new on screen takes the slot of the one it replaced.
 */
function assignSlots(shown: string[], previous: Map<string, Slot>): Map<string, Slot> {
  const next = new Map<string, Slot>();
  for (const chat of shown) {
    const slot = previous.get(chat);
    if (slot && ![...next.values()].includes(slot)) next.set(chat, slot);
  }
  const departed = [...previous.entries()].filter(([chat]) => !next.has(chat)).map(([, slot]) => slot);
  const free = [...departed, ...SLOTS].filter((slot, i, all) => all.indexOf(slot) === i);
  for (const chat of shown) {
    if (next.has(chat)) continue;
    const slot = free.find((s) => ![...next.values()].includes(s))!;
    next.set(chat, slot);
  }
  return next;
}

/**
 * A divider of the grid: the vertical one between the columns, the horizontal
 * one between the rows. Drag it, or use the arrow keys (FR-5, NFR-4). While it
 * is dragged, only the veils over the panes and the line move (direct style
 * writes: no render, no layout of the chats, NFR-1); the chats take their new
 * sizes once, on release, as the veils fade. `span` is the stretch of the
 * area the divider runs along, in px (a column divider with three panes runs
 * along the top row only).
 */
function Divider({
  axis,
  area,
  size,
  ratio,
  min,
  span,
  label,
  place,
  onChange,
  onResizing,
}: {
  axis: "x" | "y";
  area: React.RefObject<HTMLDivElement | null>;
  /** The area's width (x) or height (y). */
  size: number;
  ratio: number;
  /** The least either side may be (px). */
  min: number;
  span: [number, number];
  label: string;
  place: React.CSSProperties;
  onChange(ratio: number): void;
  onResizing(on: boolean): void;
}) {
  const x = axis === "x";
  const [dragging, setDragging] = React.useState(false);
  const veils = React.useRef<HTMLDivElement | null>(null);
  const next = React.useRef(ratio);
  const set = (value: number) => onChange(clampRatio(value, size, min));
  /** Show `value` on the veils: the first side's size and the line's place. */
  const show = (value: number) => {
    const at = clampRatio(value, size, min) * size;
    veils.current?.style.setProperty("--split-at", `${at}px`);
  };
  const cross: React.CSSProperties = x ? { top: span[0], height: span[1] - span[0], left: 0, right: 0 } : { left: span[0], width: span[1] - span[0], top: 0, bottom: 0 };
  return (
    <>
      <div
        role="separator"
        aria-orientation={x ? "vertical" : "horizontal"}
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(ratio * 100)}
        tabIndex={0}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.setPointerCapture(event.pointerId);
          next.current = ratio;
          onResizing(true);
          setDragging(true);
          // The veils mount at the current place, then follow the pointer.
          requestAnimationFrame(() => show(next.current));
        }}
        onPointerMove={(event) => {
          if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
          const rect = area.current?.getBoundingClientRect();
          if (!rect) return;
          next.current = x ? (event.clientX - rect.left) / rect.width : (event.clientY - rect.top) / rect.height;
          show(next.current);
        }}
        onPointerUp={(event) => {
          event.currentTarget.releasePointerCapture(event.pointerId);
          set(next.current);
          onResizing(false);
          // The veils fade off the chats now laid out at their new sizes.
          const element = veils.current;
          if (element && typeof element.animate === "function") {
            element.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 180, easing: "ease-out" }).finished.then(
              () => setDragging(false),
              () => setDragging(false),
            );
          } else setDragging(false);
        }}
        onDoubleClick={() => set(0.5)}
        onKeyDown={(event) => {
          const step = event.shiftKey ? 0.1 : 0.02;
          const [less, more] = x ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
          if (event.key === less) set(ratio - step);
          else if (event.key === more) set(ratio + step);
          else if (event.key === "Home") set(0);
          else if (event.key === "End") set(1);
          else return;
          event.preventDefault();
        }}
        style={{ ...place, ...(x ? { width: DIVIDER_WIDTH } : { height: DIVIDER_WIDTH }) }}
        className={cn("group relative z-10 shrink-0 outline-none", x ? "cursor-col-resize" : "cursor-row-resize")}
      >
        <div
          className={cn(
            "absolute rounded-full bg-border transition-[background-color,width,height] duration-150 group-hover:bg-brand/40 group-focus-visible:bg-brand group-active:bg-brand/60",
            x
              ? "inset-y-2 left-1/2 w-px -translate-x-1/2 group-hover:w-[3px] group-focus-visible:w-[3px]"
              : "inset-x-2 top-1/2 h-px -translate-y-1/2 group-hover:h-[3px] group-focus-visible:h-[3px]",
          )}
        />
      </div>
      {dragging && (
        <div
          ref={veils}
          aria-hidden
          className={cn("pointer-events-none absolute z-20", x ? "cursor-col-resize" : "cursor-row-resize")}
          style={{ ...cross, "--split-at": `${ratio * size}px` } as React.CSSProperties}
        >
          <div
            className={cn("absolute rounded-lg bg-background/80 ring-1 ring-border ring-inset", x ? "inset-y-0 left-0 w-(--split-at)" : "inset-x-0 top-0 h-(--split-at)")}
          />
          <div
            className={cn("absolute rounded-lg bg-background/80 ring-1 ring-border ring-inset", x ? "inset-y-0 right-0" : "inset-x-0 bottom-0")}
            style={x ? { left: `calc(var(--split-at) + ${DIVIDER_WIDTH}px)` } : { top: `calc(var(--split-at) + ${DIVIDER_WIDTH}px)` }}
          />
          <div
            className={cn("absolute rounded-full bg-brand", x ? "inset-y-2 w-[3px]" : "inset-x-2 h-[3px]")}
            style={x ? { left: `calc(var(--split-at) + ${DIVIDER_WIDTH / 2 - 1.5}px)` } : { top: `calc(var(--split-at) + ${DIVIDER_WIDTH / 2 - 1.5}px)` }}
          />
        </div>
      )}
    </>
  );
}

/** A pane's place in the grid (FR-15, FR-17): two side by side; three or four as two rows, the third spanning both columns when it is alone. */
function placement(count: number, index: number, arrangement: Arrangement): React.CSSProperties {
  if (count < 2 || arrangement === "focus") return { gridColumn: 1, gridRow: 1 };
  if (arrangement === "row") return { gridColumn: index + 1, gridRow: 1 };
  if (count === 2) return { gridColumn: index === 0 ? 1 : 3, gridRow: 1 };
  if (index < 2) return { gridColumn: index === 0 ? 1 : 3, gridRow: 1 };
  if (index === 2) return { gridColumn: count === 3 ? "1 / -1" : 1, gridRow: 3 };
  return { gridColumn: 3, gridRow: 3 };
}

/**
 * The chat area: one chat as before, two side by side, or three or four in a
 * grid of two rows (docs/features/chat-split-view/spec.md). The focused pane's
 * chat is the active one; a pointer or keyboard focus moving into another pane
 * focuses it before anything there acts (FR-4).
 */
export function SplitArea() {
  const activeId = useEngineStore((s) => s.activeId);
  const panes = useSplitStore((s) => s.panes);
  const focus = useSplitStore((s) => s.focus);
  const savedRatio = useSplitStore((s) => s.ratio);
  const savedRowRatio = useSplitStore((s) => s.rowRatio);
  const area = React.useRef<HTMLDivElement | null>(null);
  const [size, setSize] = React.useState({ width: 0, height: 0 });
  const resizing = React.useRef(false);
  const closing = React.useRef(false);

  // Size, and the drop area for drags.
  React.useLayoutEffect(() => {
    const element = area.current;
    if (!element) return;
    registerDropArea(element);
    const observer = new ResizeObserver(([entry]) => setSize({ width: entry.contentRect.width, height: entry.contentRect.height }));
    observer.observe(element);
    return () => {
      observer.disconnect();
      registerDropArea(null);
    };
  }, []);

  const shown = panes ?? [activeId ?? ""];
  const count = shown.length;
  const slotsRef = React.useRef(new Map<string, Slot>());
  const slots = assignSlots(shown, slotsRef.current);
  slotsRef.current = slots;
  const arrangement = panes ? arrange(count, size.width, size.height) : "grid";
  const narrow = arrangement === "focus";
  const ratio = clampRatio(savedRatio, size.width);
  const rowRatio = clampRatio(savedRowRatio, size.height, MIN_PANE_HEIGHT);
  const grid = Boolean(panes) && arrangement === "grid";
  const rows = grid && count >= 3;

  // Motion: each pane's place before this change, for FLIP (FR-9).
  const elements = React.useRef(new Map<Slot, HTMLElement>());
  const before = React.useRef(new Map<Slot, DOMRect>());
  const layoutKey = `${shown.map((chat) => slots.get(chat)).join(",")}|${arrangement}`;
  const lastKey = React.useRef<string | null>(null);
  React.useLayoutEffect(() => {
    const changed = lastKey.current !== null && lastKey.current !== layoutKey;
    if (changed && !resizing.current) {
      shown.forEach((chat, index) => {
        const slot = slots.get(chat)!;
        const element = elements.current.get(slot);
        if (!element) return;
        const old = before.current.get(slot);
        if (old) flip(element, old);
        else if (panes) enter(element, index === 0 ? "left" : index === 1 || arrangement !== "grid" ? "right" : "bottom");
      });
    }
    lastKey.current = layoutKey;
    const rects = new Map<Slot, DOMRect>();
    for (const [slot, element] of elements.current) rects.set(slot, element.getBoundingClientRect());
    before.current = rects;
  });
  // A resize moves the panes too: keep their places current between changes.
  React.useEffect(() => {
    const rects = new Map<Slot, DOMRect>();
    for (const [slot, element] of elements.current) rects.set(slot, element.getBoundingClientRect());
    before.current = rects;
  }, [ratio, rowRatio, size.width, size.height]);

  /** Close a pane after it has shrunk away (FR-6, FR-9). */
  const close = async (index: PaneIndex) => {
    if (closing.current || !panes) return;
    closing.current = true;
    const element = elements.current.get(slots.get(panes[index])!);
    if (element) await leave(element);
    closing.current = false;
    useSplitStore.getState().close(index);
  };

  // The grid's tracks: columns around the vertical divider, rows around the horizontal one.
  const half = DIVIDER_WIDTH / 2;
  const columns = grid
    ? size.width > 0
      ? `${ratio * size.width - half}px ${DIVIDER_WIDTH}px minmax(0, 1fr)`
      : `calc(${ratio * 100}% - ${half}px) ${DIVIDER_WIDTH}px minmax(0, 1fr)`
    : arrangement === "row"
      ? `repeat(${count}, minmax(0, 1fr))`
      : "minmax(0, 1fr)";
  const tracks = rows
    ? size.height > 0
      ? `${rowRatio * size.height - half}px ${DIVIDER_WIDTH}px minmax(0, 1fr)`
      : `1fr ${DIVIDER_WIDTH}px 1fr`
    : "minmax(0, 1fr)";
  // The column divider runs along the top row only while the bottom pane spans both columns.
  const columnSpan: [number, number] = count === 3 ? [0, rowRatio * size.height - half] : [0, size.height];

  return (
    <div
      ref={area}
      className="relative grid h-full min-h-0 min-w-0"
      style={{ gridTemplateColumns: columns, gridTemplateRows: tracks, columnGap: arrangement === "row" ? DIVIDER_WIDTH : undefined }}
    >
      {shown.map((chat, i) => {
        const index = i as PaneIndex;
        const slot = slots.get(chat)!;
        const focused = !panes || index === focus;
        const hidden = narrow && !focused;
        return (
          <section
            key={slot}
            ref={(element) => {
              if (element) elements.current.set(slot, element);
              else elements.current.delete(slot);
            }}
            aria-label={panes ? `Chat ${index + 1}` : undefined}
            onPointerDownCapture={() => {
              if (panes && !focused) useSplitStore.getState().focusIndex(index);
            }}
            onFocusCapture={() => {
              if (panes && !focused) useSplitStore.getState().focusIndex(index);
            }}
            style={hidden ? { display: "none" } : placement(count, index, arrangement)}
            className={cn(
              "relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden",
              panes && "rounded-lg",
              // The focused pane's thin accent ring (spec Q6).
              panes && !narrow && "ring-1 ring-transparent ring-inset transition-shadow duration-200",
              panes && !narrow && focused && "ring-brand/35",
            )}
          >
            {/* The same tree with one pane or several, so the chat view stays mounted when the area splits. */}
            <PaneChatContext.Provider value={panes ? chat : null}>
              {panes && <PaneHeader chat={chat} index={index} narrow={narrow} onClose={() => void close(index)} />}
              <div className="min-h-0 flex-1">
                <ChatView />
              </div>
            </PaneChatContext.Provider>
          </section>
        );
      })}
      {grid && (
        <Divider
          axis="x"
          area={area}
          size={size.width}
          ratio={ratio}
          min={MIN_PANE_WIDTH}
          span={columnSpan}
          label="Resize panes"
          place={{ gridColumn: 2, gridRow: rows ? (count === 3 ? "1" : "1 / -1") : 1 }}
          onChange={(next) => useSplitStore.getState().setRatio(next)}
          onResizing={(on) => (resizing.current = on)}
        />
      )}
      {rows && (
        <Divider
          axis="y"
          area={area}
          size={size.height}
          ratio={rowRatio}
          min={MIN_PANE_HEIGHT}
          span={[0, size.width]}
          label="Resize rows"
          place={{ gridRow: 2, gridColumn: "1 / -1" }}
          onChange={(next) => useSplitStore.getState().setRowRatio(next)}
          onResizing={(on) => (resizing.current = on)}
        />
      )}
      <DropPreview width={size.width} height={size.height} />
    </div>
  );
}
