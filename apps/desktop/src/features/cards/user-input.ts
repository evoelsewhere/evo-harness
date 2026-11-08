// When the person last used a page's picture (a press, a key, a paste): a page dialog that opens a
// moment after is the person's to answer, even while an agent drives the page
// (stores/browser-store.ts). Docs/features/floating-cards, FR-7.

const at = new Map<string, number>();

/** The person pressed a mouse button or a key on the picture of `tab`. */
export const noteUserInput = (tab: string) => void at.set(tab, Date.now());

/** Whether the person did so within `withinMs`. */
export const recentUserInput = (tab: string, withinMs = 2500) => Date.now() - (at.get(tab) ?? 0) < withinMs;
