// Shared layout rules of Evo Mates' messaging views (docs/features/evo-mates/
// ux.md 2, 3; spec FR-29, FR-58).

/**
 * A bubble's text box: it may shrink inside its row, and a long URL, path
 * or word breaks where it must instead of pushing the thread sideways. A
 * table keeps whole words: it scrolls inside its own frame.
 */
export const BUBBLE_TEXT = "min-w-0 max-w-full [overflow-wrap:anywhere] [&_table]:[overflow-wrap:normal]";

/**
 * Below this window width the mate panel floats over the conversation
 * (ux.md 2): from Evo Mates UI v2 (FR-31) it docks only from 1100 px, so a
 * folded or full sidebar always leaves the conversation room.
 */
export const PANEL_FLOAT_BELOW = 1100;

export function panelFloats(windowWidth: number): boolean {
  return windowWidth < PANEL_FLOAT_BELOW;
}
