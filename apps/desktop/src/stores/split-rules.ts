// The rules of split view: which chats the two panes show, which one has the
// focus, and how wide the left one is. Pure, so the store and its tests share
// them (docs/features/chat-split-view/spec.md, FR-1, FR-4 – FR-7, FR-11 – FR-13).

export type PaneIndex = 0 | 1;
export type Side = "left" | "right";

export interface Layout {
  /** The two panes' chats, left first; `null` while one pane (the active chat) shows. */
  panes: [string, string] | null;
  /** The focused pane: its chat is the engine store's `activeId`. */
  focus: PaneIndex;
  /** The left pane's share of the width. */
  ratio: number;
}

export const NO_SPLIT: Layout = { panes: null, focus: 0, ratio: 0.5 };
/** A pane is never narrower than this (px). */
export const MIN_PANE_WIDTH = 360;
/** The divider between the panes (px). */
export const DIVIDER_WIDTH = 8;

const other = (index: PaneIndex): PaneIndex => (index === 0 ? 1 : 0);
const indexOf = (side: Side): PaneIndex => (side === "left" ? 0 : 1);

/** Focus the pane showing `chat`, when one does. */
function focusShown(layout: Layout, chat: string): Layout | null {
  const at = layout.panes?.indexOf(chat) ?? -1;
  if (at < 0) return null;
  return at === layout.focus ? layout : { ...layout, focus: at as PaneIndex };
}

/**
 * `chat` dropped on `side`. With one pane it opens beside `active`; with two
 * it replaces the chat of the pane it lands on. A chat a pane shows only gets
 * the focus.
 */
export function openPane(layout: Layout, active: string | null, chat: string, side: Side): Layout {
  const shown = focusShown(layout, chat);
  if (shown) return shown;
  if (layout.panes) return replacePane(layout, indexOf(side), chat);
  if (!active || active === chat) return layout;
  return side === "right"
    ? { panes: [active, chat], focus: 1, ratio: NO_SPLIT.ratio }
    : { panes: [chat, active], focus: 0, ratio: NO_SPLIT.ratio };
}

/** Show `chat` in pane `index` and focus it (or focus the pane that shows it). */
export function replacePane(layout: Layout, index: PaneIndex, chat: string): Layout {
  if (!layout.panes) return layout;
  const shown = focusShown(layout, chat);
  if (shown) return shown;
  const panes: [string, string] = [...layout.panes];
  panes[index] = chat;
  return { ...layout, panes, focus: index };
}

/** The panes change sides; the focus and the widths go with them. */
export function swapPanes(layout: Layout): Layout {
  if (!layout.panes) return layout;
  return { panes: [layout.panes[1], layout.panes[0]], focus: other(layout.focus), ratio: 1 - layout.ratio };
}

/** Close pane `index`: one pane is left, showing `remaining`. */
export function closePane(layout: Layout, index: PaneIndex): { layout: Layout; remaining: string | null } {
  if (!layout.panes) return { layout, remaining: null };
  return { layout: NO_SPLIT, remaining: layout.panes[other(index)] };
}

/**
 * The active chat changed (a sidebar click, a command): the pane that shows
 * it gets the focus, or it replaces the focused pane's chat.
 */
export function followActive(layout: Layout, active: string | null): Layout {
  if (!layout.panes || !active || layout.panes[layout.focus] === active) return layout;
  return focusShown(layout, active) ?? replacePane(layout, layout.focus, active);
}

/** Close the panes whose chat is gone (archived or deleted). */
export function dropMissing(layout: Layout, alive: (chat: string) => boolean): { layout: Layout; remaining: string | null } {
  if (!layout.panes) return { layout, remaining: null };
  const [left, right] = layout.panes.map(alive);
  if (left && right) return { layout, remaining: null };
  if (!left && !right) return { layout: NO_SPLIT, remaining: null };
  return closePane(layout, left ? 1 : 0);
}

/** Keep both panes at least `MIN_PANE_WIDTH` wide in an area `width` wide. */
export function clampRatio(ratio: number, width: number): number {
  if (!(width > 0)) return ratio;
  const low = MIN_PANE_WIDTH / width;
  const high = 1 - low;
  if (low > high) return 0.5;
  return Math.min(high, Math.max(low, ratio));
}

/** Too narrow for two panes: only the focused one shows (FR-13). */
export const isNarrow = (width: number): boolean => width > 0 && width < 2 * MIN_PANE_WIDTH + DIVIDER_WIDTH;
