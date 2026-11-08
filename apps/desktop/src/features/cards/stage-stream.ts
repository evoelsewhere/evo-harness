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

const last = new Map<string, Frame>();
const listeners = new Map<string, Set<(frame: Frame) => void>>();
let started = false;

function start() {
  if (started || !inTauri) return;
  started = true;
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

/** The page closed. */
export function forgetFrames(tab: string) {
  last.delete(tab);
  listeners.delete(tab);
}
