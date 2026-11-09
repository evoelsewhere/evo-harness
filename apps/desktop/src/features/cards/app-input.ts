// The person's keys in an app's card, as `computer_input` asks for them
// (docs/features/computer-app-control FR-13): a printable character is text, any
// other key or shortcut is a key name the app control's key parser reads
// (`enter`, `ctrl+s`, `shift+tab`, `f5`).

export type AppKeyInput = { kind: "text"; text: string } | { kind: "key"; key: string };

const NAMES: Record<string, string> = {
  Enter: "enter",
  Tab: "tab",
  Escape: "escape",
  Backspace: "backspace",
  Delete: "delete",
  Insert: "insert",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  " ": "space",
};

const MODIFIERS = ["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead", "Process", "OS"];

/** The input for a key press in the card, `null` for a lone modifier. */
export function appKeyInput(event: { key: string; shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }): AppKeyInput | null {
  if (MODIFIERS.includes(event.key)) return null;
  const chord = event.ctrlKey || event.altKey || event.metaKey;
  if (event.key.length === 1 && !chord) return { kind: "text", text: event.key };
  const name = NAMES[event.key] ?? (/^F\d{1,2}$/.test(event.key) ? event.key.toLowerCase() : event.key.length === 1 ? event.key.toLowerCase() : null);
  if (!name) return null;
  const held = [event.ctrlKey || event.metaKey ? "ctrl" : null, event.altKey ? "alt" : null, event.shiftKey && event.key.length !== 1 ? "shift" : null].filter(Boolean);
  // Shift with a character key is already in the character (Ctrl+Shift+S arrives as "S").
  if (event.shiftKey && event.key.length === 1 && chord) held.push("shift");
  return { kind: "key", key: [...held, name].join("+") };
}

/** A point of the shown picture as fractions of it; `null` in the margins. */
export function toFraction(point: { x: number; y: number } | null, picture: { w: number; h: number }): { x: number; y: number } | null {
  if (!point || picture.w <= 0 || picture.h <= 0) return null;
  return { x: point.x / picture.w, y: point.y / picture.h };
}
