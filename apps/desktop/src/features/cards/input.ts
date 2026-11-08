// The person's mouse and keys on a page's picture, as DevTools input in the page's own
// coordinates (docs/features/floating-cards, FR-7, FR-9): the same coordinates the
// agent's tools use, whatever size the picture is shown at.

export interface Size {
  w: number;
  h: number;
}

/**
 * Where a point of the shown picture is on the page. The picture (`page`, in page
 * pixels) is scaled to fit `box` (the canvas element's box) and centred; `null` for a
 * point in the empty margin.
 */
export function toPagePoint(clientX: number, clientY: number, box: { left: number; top: number; width: number; height: number }, page: Size): { x: number; y: number } | null {
  if (box.width <= 0 || box.height <= 0 || page.w <= 0 || page.h <= 0) return null;
  const scale = Math.min(box.width / page.w, box.height / page.h);
  const offsetX = (box.width - page.w * scale) / 2;
  const offsetY = (box.height - page.h * scale) / 2;
  const x = (clientX - box.left - offsetX) / scale;
  const y = (clientY - box.top - offsetY) / scale;
  if (x < 0 || y < 0 || x > page.w || y > page.h) return null;
  return { x: Math.round(x * 10) / 10, y: Math.round(y * 10) / 10 };
}

const BUTTONS = ["left", "middle", "right"] as const;

/** `Input.dispatchMouseEvent` parameters for a mouse event on the picture. */
export function mouseParams(
  type: "mousePressed" | "mouseReleased" | "mouseMoved",
  point: { x: number; y: number },
  event: { button: number; buttons: number; detail: number; shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean },
) {
  const pressed = type === "mousePressed" || type === "mouseReleased";
  return {
    type,
    x: point.x,
    y: point.y,
    button: pressed ? (BUTTONS[event.button] ?? "left") : event.buttons & 1 ? "left" : event.buttons & 4 ? "middle" : event.buttons & 2 ? "right" : "none",
    // `buttons` is the state after a release, which is what DevTools expects.
    buttons: event.buttons,
    clickCount: pressed ? Math.max(event.detail, 1) : 0,
    modifiers: modifiers(event),
  };
}

/** The DevTools modifier bits: Alt 1, Ctrl 2, Meta 4, Shift 8. */
export function modifiers(event: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }) {
  return (event.altKey ? 1 : 0) | (event.ctrlKey ? 2 : 0) | (event.metaKey ? 4 : 0) | (event.shiftKey ? 8 : 0);
}

/** `Input.dispatchMouseEvent` parameters for a wheel turn; deltas are scaled to page pixels. */
export function wheelParams(point: { x: number; y: number }, event: { deltaX: number; deltaY: number; deltaMode: number }, scale: number) {
  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 400 : 1;
  return { type: "mouseWheel", x: point.x, y: point.y, deltaX: event.deltaX * unit * scale, deltaY: event.deltaY * unit * scale };
}

const VIRTUAL_KEYS: Record<string, number> = {
  Backspace: 8,
  Tab: 9,
  Enter: 13,
  Escape: 27,
  " ": 32,
  PageUp: 33,
  PageDown: 34,
  End: 35,
  Home: 36,
  ArrowLeft: 37,
  ArrowUp: 38,
  ArrowRight: 39,
  ArrowDown: 40,
  Delete: 46,
};

/**
 * The `Input.dispatchKeyEvent` pair for a key press, `null` for a key to leave to the
 * app (a shortcut with Ctrl or ⌘, a lone modifier). Printable keys carry their text;
 * Enter types a carriage return.
 */
export function keyEvents(event: { key: string; code: string; shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }) {
  if (["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead", "Process"].includes(event.key)) return null;
  if ((event.ctrlKey || event.metaKey) && event.key.length === 1) return null;
  const printable = event.key.length === 1;
  const text = printable ? event.key : event.key === "Enter" ? "\r" : undefined;
  const code = VIRTUAL_KEYS[event.key] ?? (printable ? event.key.toUpperCase().charCodeAt(0) : 0);
  const common = { key: event.key, code: event.code, windowsVirtualKeyCode: code, modifiers: modifiers(event) };
  return {
    down: { type: text ? "keyDown" : "rawKeyDown", ...common, ...(text ? { text, unmodifiedText: text } : {}) },
    up: { type: "keyUp", ...common },
  };
}
