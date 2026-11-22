// Where a dragged chat or pane lands in the chat area, and the preview that
// shows it (docs/features/chat-split-view/spec.md, FR-2, FR-7, FR-13,
// FR-15 – FR-17). Pure: the drag controller, the split area and their tests
// share it.

import { DIVIDER_WIDTH, MAX_PANES, arrange, type Arrangement, type PaneIndex } from "@/stores/split-rules";

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** What is dragged: a chat (a sidebar row) or a pane (its header). */
export type DragPayload = { kind: "chat"; chat: string; title?: string } | { kind: "pane"; index: PaneIndex };

/** What a drop does: a new pane at a place in reading order, a chat in place of a pane's, two panes trading places. */
export type DropTarget = { kind: "open"; at: number } | { kind: "replace"; index: PaneIndex } | { kind: "swap"; into: PaneIndex };

export interface AreaLayout {
  panes: string[] | null;
  focus: PaneIndex;
  ratio: number;
  rowRatio: number;
  /** How the panes fit the area now. */
  arrangement: Arrangement;
}

/** How near a pane's edge a drop must be to add a pane there (px, at most; and a share of the pane). */
const EDGE = 96;
const EDGE_SHARE = 0.2;

/**
 * Each pane's box in an area of `width` × `height`, relative to the area, for
 * `count` panes: the grid cells (the divider's width between them), a row of
 * equal panes, or the whole area for the focused one.
 */
export function paneRects(count: number, arrangement: Arrangement, ratio: number, rowRatio: number, width: number, height: number): Rect[] {
  const full: Rect = { left: 0, top: 0, width, height };
  if (count < 2 || arrangement === "focus") return Array.from({ length: Math.max(1, count) }, () => full);
  if (arrangement === "row") {
    const each = (width - (count - 1) * DIVIDER_WIDTH) / count;
    return Array.from({ length: count }, (_, i) => ({ left: i * (each + DIVIDER_WIDTH), top: 0, width: each, height }));
  }
  const half = DIVIDER_WIDTH / 2;
  const leftWidth = ratio * width - half;
  const rightLeft = ratio * width + half;
  const rightWidth = width - rightLeft;
  if (count === 2) return [{ left: 0, top: 0, width: leftWidth, height }, { left: rightLeft, top: 0, width: rightWidth, height }];
  const topHeight = rowRatio * height - half;
  const bottomTop = rowRatio * height + half;
  const bottomHeight = height - bottomTop;
  const rects: Rect[] = [
    { left: 0, top: 0, width: leftWidth, height: topHeight },
    { left: rightLeft, top: 0, width: rightWidth, height: topHeight },
    count === 3 ? { left: 0, top: bottomTop, width, height: bottomHeight } : { left: 0, top: bottomTop, width: leftWidth, height: bottomHeight },
  ];
  if (count === 4) rects.push({ left: rightLeft, top: bottomTop, width: rightWidth, height: bottomHeight });
  return rects;
}

const distance = (r: Rect, x: number, y: number): number => {
  const dx = Math.max(r.left - x, 0, x - (r.left + r.width));
  const dy = Math.max(r.top - y, 0, y - (r.top + r.height));
  return Math.hypot(dx, dy);
};

export function dropTarget(area: Rect, x: number, y: number, layout: AreaLayout, payload: DragPayload): DropTarget | null {
  if (x < area.left || x > area.left + area.width || y < area.top || y > area.top + area.height) return null;
  const { panes } = layout;
  if (!panes) {
    if (payload.kind === "pane") return null;
    return { kind: "open", at: x - area.left < area.width / 2 ? 0 : 1 };
  }
  if (layout.arrangement === "focus") return payload.kind === "pane" ? null : { kind: "replace", index: layout.focus };
  const rects = paneRects(panes.length, layout.arrangement, layout.ratio, layout.rowRatio, area.width, area.height);
  const px = x - area.left;
  const py = y - area.top;
  // The pane under the pointer; over a divider, the nearest one.
  let over = 0;
  for (let i = 1; i < rects.length; i++) if (distance(rects[i], px, py) < distance(rects[over], px, py)) over = i;
  if (payload.kind === "pane") return over === payload.index ? null : { kind: "swap", into: over };
  if (panes.length >= MAX_PANES) return { kind: "replace", index: over };
  // Near a side edge a new pane opens there; near the bottom one opens last; else the pane's chat is replaced.
  const r = rects[over];
  const edgeX = Math.min(EDGE, r.width * EDGE_SHARE);
  const edgeY = Math.min(EDGE, r.height * EDGE_SHARE);
  if (px - r.left < edgeX) return { kind: "open", at: over };
  if (r.left + r.width - px < edgeX) return { kind: "open", at: over + 1 };
  const lastRow = panes.length === 2 ? 0 : 2;
  if (r.top + r.height - py < edgeY && over >= lastRow) return { kind: "open", at: panes.length };
  return { kind: "replace", index: over };
}

/** The preview box, relative to the area: the place the drop will fill. */
export function previewRect(area: Rect, target: DropTarget, layout: AreaLayout): Rect {
  const full = { left: 0, top: 0, width: area.width, height: area.height };
  const count = layout.panes?.length ?? 1;
  const ratio = layout.panes ? layout.ratio : 0.5;
  const rowRatio = layout.panes ? layout.rowRatio : 0.5;
  if (target.kind === "open") {
    const next = count + 1;
    const arrangement = arrange(next, area.width, area.height);
    return paneRects(next, arrangement, ratio, rowRatio, area.width, area.height)[target.at] ?? full;
  }
  if (layout.arrangement === "focus") return full;
  const rects = paneRects(count, layout.arrangement, ratio, rowRatio, area.width, area.height);
  return rects[target.kind === "replace" ? target.index : target.into] ?? full;
}
