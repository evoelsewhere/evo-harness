// Where a dragged chat or pane lands in the chat area, and the preview that
// shows it (docs/features/chat-split-view/spec.md, FR-2, FR-7, FR-13). Pure:
// the drag controller and its tests share it.

import type { PaneIndex, Side } from "@/stores/split-rules";

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** What is dragged: a chat (a sidebar row) or a pane (its header). */
export type DragPayload = { kind: "chat"; chat: string; title?: string } | { kind: "pane"; index: PaneIndex };

/** What a drop does. */
export type DropTarget = { kind: "open"; side: Side } | { kind: "replace"; index: PaneIndex } | { kind: "swap"; into: PaneIndex };

export interface AreaLayout {
  panes: [string, string] | null;
  focus: PaneIndex;
  ratio: number;
  /** Only the focused pane shows (FR-13). */
  narrow: boolean;
}

export function dropTarget(area: Rect, x: number, y: number, layout: AreaLayout, payload: DragPayload): DropTarget | null {
  if (x < area.left || x > area.left + area.width || y < area.top || y > area.top + area.height) return null;
  const left = x - area.left < area.width * (layout.panes && !layout.narrow ? layout.ratio : 0.5);
  if (payload.kind === "pane") {
    if (!layout.panes || layout.narrow) return null;
    const over: PaneIndex = left ? 0 : 1;
    return over === payload.index ? null : { kind: "swap", into: over };
  }
  if (!layout.panes) return { kind: "open", side: left ? "left" : "right" };
  if (layout.narrow) return { kind: "replace", index: layout.focus };
  return { kind: "replace", index: left ? 0 : 1 };
}

/** The preview box, relative to the area. */
export function previewRect(area: Rect, target: DropTarget, layout: AreaLayout): Rect {
  const full = { left: 0, top: 0, width: area.width, height: area.height };
  if (layout.narrow && target.kind !== "open") return full;
  const split = layout.panes ? area.width * layout.ratio : area.width / 2;
  const pane = (index: PaneIndex): Rect =>
    index === 0 ? { ...full, width: split } : { ...full, left: split, width: area.width - split };
  switch (target.kind) {
    case "open":
      return target.side === "left" ? { ...full, width: area.width / 2 } : { ...full, left: area.width / 2, width: area.width / 2 };
    case "replace":
      return pane(target.index);
    case "swap":
      // The pane goes where the other one is: that place lights up.
      return pane(target.into);
  }
}
