// The pictures of staged pages (docs/features/floating-cards, FR-9): the shell sends
// `browser://frame` for a page that something shows; a canvas paints the latest one.
// Frames are kept out of React state (about ten a second, 16 KB and more each):
// a canvas subscribes to its page's frames and draws them itself.

import { listen } from "@tauri-apps/api/event";
import { inTauri } from "@/lib/platform";

export interface Frame {
  tab: string;
  seq: number;
  /** JPEG, base64. */
  data: string;
  width: number;
  height: number;
}

/** `browser://pointer`: where an agent tool puts the mouse on a page (its virtual cursor). */
export interface Pointer {
  tab: string;
  chat: string | null;
  kind: "move" | "down" | "up" | "wheel";
  x: number;
  y: number;
}

const lastPointers = new Map<string, Pointer & { at: number }>();
const pointerListeners = new Map<string, Set<(pointer: Pointer) => void>>();
const last = new Map<string, Frame>();
const listeners = new Map<string, Set<(frame: Frame) => void>>();
let started = false;

function start() {
  if (started || !inTauri) return;
  started = true;
  void listen<Pointer>("browser://pointer", (event) => {
    const pointer = event.payload;
    lastPointers.set(pointer.tab, { ...pointer, at: Date.now() });
    pointerListeners.get(pointer.tab)?.forEach((listener) => listener(pointer));
  }).catch(() => undefined);
  void listen<Frame>("browser://frame", (event) => {
    const frame = event.payload;
    last.set(frame.tab, frame);
    listeners.get(frame.tab)?.forEach((listener) => listener(frame));
  }).catch(() => undefined);
}

/** The newest frame of `tab` seen so far, if any (a canvas that mounts later paints it at once). */
export const lastFrame = (tab: string) => last.get(tab);

/** Call `listener` with every frame of `tab`; returns how to stop. */
export function subscribeFrames(tab: string, listener: (frame: Frame) => void): () => void {
  start();
  const set = listeners.get(tab) ?? new Set();
  set.add(listener);
  listeners.set(tab, set);
  return () => {
    set.delete(listener);
    if (set.size === 0) listeners.delete(tab);
  };
}

/** The newest pointer event of `tab` and when it came (ms), if any. */
export const lastPointer = (tab: string) => lastPointers.get(tab);

/** Call `listener` with every pointer event of `tab`; returns how to stop. */
export function subscribePointer(tab: string, listener: (pointer: Pointer) => void): () => void {
  start();
  const set = pointerListeners.get(tab) ?? new Set();
  set.add(listener);
  pointerListeners.set(tab, set);
  return () => {
    set.delete(listener);
    if (set.size === 0) pointerListeners.delete(tab);
  };
}

/** The page closed. */
export function forgetFrames(tab: string) {
  last.delete(tab);
  listeners.delete(tab);
  lastPointers.delete(tab);
  pointerListeners.delete(tab);
}
