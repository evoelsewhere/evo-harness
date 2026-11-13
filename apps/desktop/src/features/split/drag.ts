// Dragging a chat (a sidebar row) or a pane (its header) onto the chat area
// (docs/features/chat-split-view/spec.md, FR-2, FR-7). Pointer events, not
// HTML drag and drop: the window takes native file drops for the composer,
// which keeps the webview's own drag events from firing on Windows. Only
// drags started here are known, so nothing from outside the app can land.

import { create } from "zustand";
import { useSplitStore } from "@/stores/split-store";
import { isNarrow } from "@/stores/split-rules";
import { dropTarget, type DragPayload, type DropTarget } from "./drop-geometry";

interface DragState {
  payload: DragPayload | null;
  /** The pointer, in client pixels. */
  x: number;
  y: number;
  /** Where a drop would land now. */
  target: DropTarget | null;
}

export const useDragStore = create<DragState>(() => ({ payload: null, x: 0, y: 0, target: null }));

/** The chat area, registered by the split area while it is on screen. */
let area: HTMLElement | null = null;
export function registerDropArea(element: HTMLElement | null): void {
  area = element;
}

/** How far the pointer moves before a press becomes a drag (px). */
const THRESHOLD = 6;

function targetAt(x: number, y: number, payload: DragPayload): DropTarget | null {
  if (!area) return null;
  const rect = area.getBoundingClientRect();
  const { panes, focus, ratio } = useSplitStore.getState();
  return dropTarget(rect, x, y, { panes, focus, ratio, narrow: isNarrow(rect.width) }, payload);
}

function land(payload: DragPayload, target: DropTarget): void {
  const split = useSplitStore.getState();
  if (target.kind === "swap") split.swap();
  else if (payload.kind !== "chat") return;
  else if (target.kind === "open") void split.open(payload.chat, target.side);
  else void split.replace(target.index, payload.chat);
}

/**
 * A press on something draggable: once the pointer moves past the
 * threshold it becomes a drag; Esc or a drop outside the area cancels it.
 * The click that ends a drag does not reach the row.
 */
export function startDrag(event: React.PointerEvent, payload: DragPayload): void {
  if (event.button !== 0 || event.pointerType === "touch") return;
  const startX = event.clientX;
  const startY = event.clientY;
  let dragging = false;

  const move = (e: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD) return;
      dragging = true;
      document.documentElement.dataset.evoDragging = "";
    }
    e.preventDefault();
    useDragStore.setState({ payload, x: e.clientX, y: e.clientY, target: targetAt(e.clientX, e.clientY, payload) });
  };
  const end = (drop: boolean) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("keydown", key, true);
    delete document.documentElement.dataset.evoDragging;
    const { target } = useDragStore.getState();
    useDragStore.setState({ payload: null, target: null });
    if (!dragging) return;
    // The click that follows the pointer-up is the drag's, not the row's.
    const swallow = (e: MouseEvent) => {
      e.stopPropagation();
      e.preventDefault();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    if (drop && target) land(payload, target);
  };
  const up = () => end(true);
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      end(false);
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("keydown", key, true);
}
