// The rules of split view: which chats the panes show (one to four, in reading
// order), which one has the focus, how the grid is divided, and how it
// arranges itself in the room it has. Pure, so the store and its tests share
// them (docs/features/chat-split-view/spec.md, FR-1, FR-4 – FR-7, FR-11 – FR-13,
// FR-15 – FR-18).

export type PaneIndex = number;

export interface Layout {
  /** The panes' chats in reading order (left to right, top to bottom); `null` while one pane (the active chat) shows. */
  panes: string[] | null;
  /** The focused pane: its chat is the engine store's `activeId`. */
  focus: PaneIndex;
  /** The left column's share of the width (one divider for every row). */
  ratio: number;
  /** The top row's share of the height, with three or four panes. */
  rowRatio: number;
}

export const NO_SPLIT: Layout = { panes: null, focus: 0, ratio: 0.5, rowRatio: 0.5 };
/** At most this many panes at once. */
export const MAX_PANES = 4;
/** A pane is never narrower than this (px). */
export const MIN_PANE_WIDTH = 360;
/** A pane is never shorter than this (px) in a grid with two rows. */
export const MIN_PANE_HEIGHT = 300;
/** The divider between the panes (px). */
export const DIVIDER_WIDTH = 8;

/**
 * How `count` panes lay out in a `width` × `height` area: the grid (two panes
 * side by side; three or four as two rows), one row when the area is too low
 * for two rows, or only the focused pane (FR-13, FR-17).
 */
export type Arrangement = "grid" | "row" | "focus";

export function arrange(count: number, width: number, height: number): Arrangement {
  if (count < 2 || !(width > 0) || !(height > 0)) return "grid";
  if (width < 2 * MIN_PANE_WIDTH + DIVIDER_WIDTH) return "focus";
  if (count === 2 || height >= 2 * MIN_PANE_HEIGHT + DIVIDER_WIDTH) return "grid";
  return width >= count * MIN_PANE_WIDTH + (count - 1) * DIVIDER_WIDTH ? "row" : "focus";
}

/** Focus the pane showing `chat`, when one does. */
function focusShown(layout: Layout, chat: string): Layout | null {
  const at = layout.panes?.indexOf(chat) ?? -1;
  if (at < 0) return null;
  return at === layout.focus ? layout : { ...layout, focus: at };
}

/**
 * `chat` dropped at place `at` (0 – count). With one pane it opens beside
 * `active`: `at` 0 on its left, otherwise on its right. With several it
 * opens as a new pane at that place in reading order, or, when the grid is
 * full, replaces the pane there. A chat a pane shows only gets the focus.
 */
export function openPane(layout: Layout, active: string | null, chat: string, at: number): Layout {
  const shown = focusShown(layout, chat);
  if (shown) return shown;
  if (layout.panes) {
    if (layout.panes.length >= MAX_PANES) return replacePane(layout, Math.min(at, layout.panes.length - 1), chat);
    const place = Math.max(0, Math.min(at, layout.panes.length));
    const panes = [...layout.panes];
    panes.splice(place, 0, chat);
    return { ...layout, panes, focus: place };
  }
  if (!active || active === chat) return layout;
  return at <= 0
    ? { ...NO_SPLIT, panes: [chat, active], focus: 0 }
    : { ...NO_SPLIT, panes: [active, chat], focus: 1 };
}

/** Show `chat` in pane `index` and focus it (or focus the pane that shows it). */
export function replacePane(layout: Layout, index: PaneIndex, chat: string): Layout {
  if (!layout.panes) return layout;
  const shown = focusShown(layout, chat);
  if (shown) return shown;
  const panes = [...layout.panes];
  panes[index] = chat;
  return { ...layout, panes, focus: index };
}

/**
 * Panes `a` and `b` change places; the focus goes with its chat. With two
 * panes the widths go with them, too.
 */
export function swapPanes(layout: Layout, a = 0, b = 1): Layout {
  if (!layout.panes || a === b || !(a in layout.panes) || !(b in layout.panes)) return layout;
  const panes = [...layout.panes];
  [panes[a], panes[b]] = [panes[b], panes[a]];
  const focus = layout.focus === a ? b : layout.focus === b ? a : layout.focus;
  return { ...layout, panes, focus, ratio: panes.length === 2 ? 1 - layout.ratio : layout.ratio };
}

/**
 * Close pane `index`. With two panes one is left and the layout ends; with
 * more, the rest keep their order. `remaining` is the chat that has the focus
 * afterwards.
 */
export function closePane(layout: Layout, index: PaneIndex): { layout: Layout; remaining: string | null } {
  const { panes } = layout;
  if (!panes || !(index in panes)) return { layout, remaining: null };
  const rest = panes.filter((_, at) => at !== index);
  if (rest.length < 2) return { layout: NO_SPLIT, remaining: rest[0] ?? null };
  const focus = layout.focus > index ? layout.focus - 1 : Math.min(layout.focus, rest.length - 1);
  return { layout: { ...layout, panes: rest, focus }, remaining: rest[focus] };
}

/**
 * The active chat changed (a sidebar click, a command): the pane that shows
 * it gets the focus, or it replaces the focused pane's chat.
 */
export function followActive(layout: Layout, active: string | null): Layout {
  if (!layout.panes || !active || layout.panes[layout.focus] === active) return layout;
  return focusShown(layout, active) ?? replacePane(layout, layout.focus, active);
}

/**
 * Close the panes whose chat is gone (archived or deleted). `remaining` is
 * the chat that has the focus afterwards, when anything closed.
 */
export function dropMissing(layout: Layout, alive: (chat: string) => boolean): { layout: Layout; remaining: string | null } {
  const { panes } = layout;
  if (!panes || panes.every(alive)) return { layout, remaining: null };
  const focused = panes[layout.focus];
  const rest = panes.filter(alive);
  if (rest.length === 0) return { layout: NO_SPLIT, remaining: null };
  if (rest.length === 1) return { layout: NO_SPLIT, remaining: rest[0] };
  const kept = rest.indexOf(focused);
  const focus = kept >= 0 ? kept : Math.min(layout.focus, rest.length - 1);
  return { layout: { ...layout, panes: rest, focus }, remaining: rest[focus] };
}

/** Keep both sides of a divider at least `min` wide in `size` (px): `ratio` is the first side's share. */
export function clampRatio(ratio: number, size: number, min = MIN_PANE_WIDTH): number {
  if (!(size > 0)) return ratio;
  const low = min / size;
  const high = 1 - low;
  if (low > high) return 0.5;
  return Math.min(high, Math.max(low, ratio));
}

/** Too small for two panes side by side: only the focused one shows (FR-13). */
export const isNarrow = (width: number): boolean => width > 0 && width < 2 * MIN_PANE_WIDTH + DIVIDER_WIDTH;
