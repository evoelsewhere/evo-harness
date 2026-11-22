// Split view motion: an underdamped spring drawn with `linear()` easing,
// played with the Web Animations API (no animation library, no React render
// per frame). docs/features/chat-split-view/spec.md, FR-9, NFR-1, NFR-6.

/** A spring that settles with a small overshoot (about 5 %). */
export const SPRING =
  "linear(0, 0.009, 0.035 2.1%, 0.141 4.4%, 0.723 12.9%, 0.938 16.7%, 1.017 20.5%, 1.043 24.5%, 1.035 28.4%, 1.007 36%, 0.998 43.7%, 1.001 59.9%, 1)";
/** How long a spring plays (ms); within FR-9's 400 ms. */
export const SPRING_MS = 380;
/** A short ease for fades and exits. */
export const EASE_OUT = "cubic-bezier(.2, .8, .2, 1)";

/** The system asks for less motion: panes change at once (FR-9). */
export const reducedMotion = (): boolean =>
  typeof window !== "undefined" && Boolean(window.matchMedia?.("(prefers-reduced-motion: reduce)").matches);

/** Play `keyframes` on `element` with the spring, unless motion is reduced. */
export function spring(element: Element, keyframes: Keyframe[], duration = SPRING_MS): Animation | null {
  if (reducedMotion() || typeof element.animate !== "function") return null;
  return element.animate(keyframes, { duration, easing: SPRING });
}

/** A new pane slides in from its side (or from below) and fades in. */
export function enter(element: Element, from: "left" | "right" | "bottom"): void {
  const offset = from === "bottom" ? "translateY(40px)" : `translateX(${from === "left" ? -40 : 40}px)`;
  spring(element, [
    { transform: `${offset} scale(.985)`, opacity: 0 },
    { transform: "none", opacity: 1 },
  ]);
}

/** A pane leaves: it shrinks a little and fades out; resolves when done. */
export function leave(element: Element): Promise<void> {
  if (reducedMotion() || typeof element.animate !== "function") return Promise.resolve();
  const animation = element.animate(
    [
      { transform: "none", opacity: 1 },
      { transform: "scale(.97)", opacity: 0 },
    ],
    { duration: 200, easing: EASE_OUT, fill: "forwards" },
  );
  return animation.finished.then(
    () => undefined,
    () => undefined,
  );
}

/**
 * FLIP: `element` moved from `before` to where it is now. It is drawn back
 * at its old place and springs to the new one. A pane that grew is revealed
 * from its old size with a clip, so its text reflows once, not every frame.
 */
export function flip(element: HTMLElement, before: DOMRect): void {
  const after = element.getBoundingClientRect();
  const dx = before.left - after.left;
  const dy = before.top - after.top;
  const grewX = after.width - before.width;
  const grewY = after.height - before.height;
  const moved = Math.abs(dx) >= 1 || Math.abs(dy) >= 1;
  if (!moved && grewX < 1 && grewY < 1) return;
  const start: Keyframe = {};
  if (moved) start.transform = `translate(${dx}px, ${dy}px)`;
  if (grewX >= 1 || grewY >= 1) {
    // The part that is new shows last: from the side it grew towards.
    const hiddenX = grewX >= 1 ? Math.max(0, (grewX / after.width) * 100) : 0;
    const hiddenY = grewY >= 1 ? Math.max(0, (grewY / after.height) * 100) : 0;
    const top = dy > 0 ? hiddenY : 0;
    const bottom = dy > 0 ? 0 : hiddenY;
    const left = dx > 0 ? hiddenX : 0;
    const right = dx > 0 ? 0 : hiddenX;
    start.clipPath = `inset(${top}% ${right}% ${bottom}% ${left}%)`;
  }
  spring(element, [start, { transform: "none", clipPath: "inset(0 0 0 0)" }]);
}
