// The Evo Mates window's layout rules (docs/features/evo-mates-ui-v2/spec.md,
// FR-1, FR-8, FR-31; plan 3.5): when the sidebar is a rail, when the profile
// docks, and the order Alt+↑/↓ walks.

/** Below this window width the sidebar is a rail of faces (FR-1; evo-mates QA H4). */
export const RAIL_BELOW = 760;
/** From this window width the profile docks beside the conversation; under it, it floats (FR-31). */
export const DOCK_FROM = 1100;

export const SIDEBAR_FULL = 280;
export const SIDEBAR_RAIL = 68;

export type SidebarForm = "full" | "rail";

export function sidebarForm(windowWidth: number, folded: boolean): SidebarForm {
  return folded || windowWidth < RAIL_BELOW ? "rail" : "full";
}

/** The width rule alone forces the rail: the fold button cannot unfold it. */
export function railForced(windowWidth: number): boolean {
  return windowWidth < RAIL_BELOW;
}

export function panelDocks(windowWidth: number): boolean {
  return windowWidth >= DOCK_FROM;
}

/** One place in the sidebar's order. */
export type Place =
  | { kind: "today" }
  | { kind: "files" }
  | { kind: "mate"; id: string; chat: string }
  | { kind: "channel"; id: string; chat: string };

export function placeKey(place: Place): string {
  return place.kind === "mate" || place.kind === "channel" ? `${place.kind}:${place.id}` : place.kind;
}

/**
 * The sidebar's order (FR-8): Today, Files, pinned mates, the other mates,
 * channels; hidden ones are left out.
 */
export function sidebarOrder(
  mates: { id: string; chatId: string; pinned: boolean; hidden: boolean }[],
  channels: { id: string; chatId: string; hidden: boolean }[],
): Place[] {
  const shown = mates.filter((m) => !m.hidden);
  return [
    { kind: "today" },
    { kind: "files" },
    ...shown.filter((m) => m.pinned).map((m) => ({ kind: "mate" as const, id: m.id, chat: m.chatId })),
    ...shown.filter((m) => !m.pinned).map((m) => ({ kind: "mate" as const, id: m.id, chat: m.chatId })),
    ...channels.filter((c) => !c.hidden).map((c) => ({ kind: "channel" as const, id: c.id, chat: c.chatId })),
  ];
}

/**
 * The place `step` away from `current` (Alt+↑/↓), or, with `attention`,
 * the next one that is unread or waiting (Alt+Shift+↑/↓), wrapping round.
 */
export function stepPlace(
  order: Place[],
  current: string | null,
  step: 1 | -1,
  attention?: (chat: string) => boolean,
): Place | null {
  if (!order.length) return null;
  const at = current ? order.findIndex((p) => placeKey(p) === current) : -1;
  for (let i = 1; i <= order.length; i++) {
    const index = (((at + step * i) % order.length) + order.length) % order.length;
    const place = order[index];
    if (!attention) return place;
    if ((place.kind === "mate" || place.kind === "channel") && attention(place.chat)) return place;
  }
  return null;
}
