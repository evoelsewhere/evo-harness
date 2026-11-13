import * as React from "react";
import { ChatView } from "@/features/chat/chat-view";
import { cn } from "@/lib/utils";
import { PaneChatContext, useEngineStore } from "@/stores/engine-store";
import { DIVIDER_WIDTH, clampRatio, isNarrow, type PaneIndex } from "@/stores/split-rules";
import { useSplitStore } from "@/stores/split-store";
import { registerDropArea } from "./drag";
import { DropPreview } from "./drop-overlay";
import { enter, flip, leave } from "./motion";
import { PaneHeader } from "./pane-header";

type Slot = "a" | "b";

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
  const free = [...departed, "a" as Slot, "b" as Slot].filter((slot, i, all) => all.indexOf(slot) === i);
  for (const chat of shown) {
    if (next.has(chat)) continue;
    const slot = free.find((s) => ![...next.values()].includes(s))!;
    next.set(chat, slot);
  }
  return next;
}

/**
 * The divider between the panes: drag it, or use the arrow keys (FR-5,
 * NFR-4). While it is dragged, only the veils over the panes and the line
 * move (direct style writes: no render, no layout of the chats, NFR-1); the
 * chats take their new widths once, on release, as the veils fade.
 */
function Divider({ area, width, ratio, onResizing }: { area: React.RefObject<HTMLDivElement | null>; width: number; ratio: number; onResizing(on: boolean): void }) {
  const setRatio = useSplitStore((s) => s.setRatio);
  const [dragging, setDragging] = React.useState(false);
  const veils = React.useRef<HTMLDivElement | null>(null);
  const next = React.useRef(ratio);
  const set = (value: number) => setRatio(clampRatio(value, width));
  /** Show `value` on the veils: the left one's width and the line's place. */
  const show = (value: number) => {
    const at = clampRatio(value, width) * width;
    veils.current?.style.setProperty("--split-at", `${at}px`);
  };
  return (
    <>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize panes"
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
          next.current = (event.clientX - rect.left) / rect.width;
          show(next.current);
        }}
        onPointerUp={(event) => {
          event.currentTarget.releasePointerCapture(event.pointerId);
          set(next.current);
          onResizing(false);
          // The veils fade off the chats now laid out at their new widths.
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
          if (event.key === "ArrowLeft") set(ratio - step);
          else if (event.key === "ArrowRight") set(ratio + step);
          else if (event.key === "Home") set(0);
          else if (event.key === "End") set(1);
          else return;
          event.preventDefault();
        }}
        style={{ width: DIVIDER_WIDTH }}
        className="group relative z-10 shrink-0 cursor-col-resize outline-none"
      >
        <div className="absolute inset-y-2 left-1/2 w-px -translate-x-1/2 rounded-full bg-border transition-[background-color,width] duration-150 group-hover:w-[3px] group-hover:bg-brand/40 group-focus-visible:w-[3px] group-focus-visible:bg-brand group-active:bg-brand/60" />
      </div>
      {dragging && (
        <div
          ref={veils}
          aria-hidden
          className="pointer-events-none absolute inset-0 z-20 cursor-col-resize"
          style={{ "--split-at": `${ratio * width}px` } as React.CSSProperties}
        >
          <div className="absolute inset-y-0 left-0 w-(--split-at) rounded-lg bg-background/80 ring-1 ring-border ring-inset" />
          <div
            className="absolute inset-y-0 right-0 rounded-lg bg-background/80 ring-1 ring-border ring-inset"
            style={{ left: `calc(var(--split-at) + ${DIVIDER_WIDTH}px)` }}
          />
          <div
            className="absolute inset-y-2 w-[3px] rounded-full bg-brand"
            style={{ left: `calc(var(--split-at) + ${DIVIDER_WIDTH / 2 - 1.5}px)` }}
          />
        </div>
      )}
    </>
  );
}

/**
 * The chat area: one chat as before, or two side by side (docs/features/
 * chat-split-view/spec.md). The focused pane's chat is the active one; a
 * pointer or keyboard focus moving into the other pane focuses it before
 * anything there acts (FR-4).
 */
export function SplitArea() {
  const activeId = useEngineStore((s) => s.activeId);
  const panes = useSplitStore((s) => s.panes);
  const focus = useSplitStore((s) => s.focus);
  const savedRatio = useSplitStore((s) => s.ratio);
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
  const slotsRef = React.useRef(new Map<string, Slot>());
  const slots = assignSlots(shown, slotsRef.current);
  slotsRef.current = slots;
  const narrow = Boolean(panes) && isNarrow(size.width);
  const ratio = clampRatio(savedRatio, size.width);

  // Motion: each pane's place before this change, for FLIP (FR-9).
  const elements = React.useRef(new Map<Slot, HTMLElement>());
  const before = React.useRef(new Map<Slot, DOMRect>());
  const layoutKey = `${shown.map((chat) => slots.get(chat)).join(",")}|${narrow}`;
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
        else if (panes) enter(element, index === 0 ? "left" : "right");
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
  }, [ratio, size.width]);

  /** Close a pane after it has shrunk away (FR-6, FR-9). */
  const close = async (index: PaneIndex) => {
    if (closing.current || !panes) return;
    closing.current = true;
    const element = elements.current.get(slots.get(panes[index])!);
    if (element) await leave(element);
    closing.current = false;
    useSplitStore.getState().close(index);
  };

  return (
    <div ref={area} className="relative flex h-full min-h-0 min-w-0">
      {shown.map((chat, i) => {
        const index = i as PaneIndex;
        const slot = slots.get(chat)!;
        const focused = !panes || index === focus;
        const hidden = narrow && !focused;
        return (
          <React.Fragment key={slot}>
            {panes && index === 1 && !narrow && (
              <Divider area={area} width={size.width} ratio={ratio} onResizing={(on) => (resizing.current = on)} />
            )}
            <section
              ref={(element) => {
                if (element) elements.current.set(slot, element);
                else elements.current.delete(slot);
              }}
              aria-label={panes ? (index === 0 ? "Left chat" : "Right chat") : undefined}
              onPointerDownCapture={() => {
                if (panes && !focused) useSplitStore.getState().focusIndex(index);
              }}
              onFocusCapture={() => {
                if (panes && !focused) useSplitStore.getState().focusIndex(index);
              }}
              style={
                hidden
                  ? { display: "none" }
                  : panes && !narrow && index === 0
                    ? { width: `calc(${ratio * 100}% - ${DIVIDER_WIDTH / 2}px)` }
                    : undefined
              }
              className={cn(
                "relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden",
                (!panes || narrow || index === 1) && "flex-1",
                panes && "rounded-lg",
                // The focused pane's thin accent ring (spec Q6).
                panes && !narrow && "ring-1 ring-transparent ring-inset transition-shadow duration-200",
                panes && !narrow && focused && "ring-brand/35",
              )}
            >
              {/* The same tree with one pane or two, so the chat view stays mounted when the area splits. */}
              <PaneChatContext.Provider value={panes ? chat : null}>
                {panes && <PaneHeader chat={chat} index={index} narrow={narrow} onClose={() => void close(index)} />}
                <div className="min-h-0 flex-1">
                  <ChatView />
                </div>
              </PaneChatContext.Provider>
            </section>
          </React.Fragment>
        );
      })}
      <DropPreview width={size.width} height={size.height} />
    </div>
  );
}
