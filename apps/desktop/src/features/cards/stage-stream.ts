// The pictures of staged pages (docs/features/floating-cards, FR-9): the shell sends
// `browser://frame` for a page that something shows; a canvas paints the latest one.
// The app a chat's agent controls streams the same way (`computer://frame`, keyed by
// its surface `app:<chat>`; docs/features/computer-app-control FR-10).
// Frames are kept out of React state (about ten a second, 130 KB each at 1280×800):
// a canvas subscribes to its page's frames and draws them itself.
//
// The frames come as raw bytes down a channel this window hands the shell
// (`stage_frames`, src-tauri/src/frames.rs), not as events: an event brings each
// picture as base64 text inside a script, and the renderer kept every script and
// its copies until the next garbage collection, hundreds of MB with a few cards.
// The events still come until the channel is in place.

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri } from "@/lib/platform";

export interface Frame {
  tab: string;
  seq: number;
  /** The JPEG. */
  image: Blob;
  width: number;
  height: number;
}

/** A base64 JPEG (an event's `data`) as a blob. */
export function jpegBlob(base64: string): Blob {
  const text = atob(base64);
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
  return new Blob([bytes], { type: "image/jpeg" });
}

/** The bytes of a frame from the shell: header length (u32, little-endian), header JSON, JPEG. */
export function readPacket(buffer: ArrayBuffer): { header: Record<string, unknown>; image: Blob } | null {
  if (buffer.byteLength < 4) return null;
  const length = new DataView(buffer).getUint32(0, true);
  if (4 + length > buffer.byteLength) return null;
  try {
    const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 4, length)));
    if (!header || typeof header !== "object") return null;
    return { header, image: new Blob([new Uint8Array(buffer, 4 + length)], { type: "image/jpeg" }) };
  } catch {
    return null;
  }
}

/**
 * Decode `frame` and hand the picture to `paint`; its memory goes as soon as `paint` returns
 * (an `<img>` per frame keeps every decoded picture until a garbage collection).
 */
export async function paintFrame(frame: Frame, paint: (picture: ImageBitmap) => void): Promise<void> {
  const picture = await createImageBitmap(frame.image);
  try {
    paint(picture);
  } finally {
    picture.close();
  }
}

/** `browser://pointer`: where an agent tool puts the mouse on a page (its virtual cursor). */
export interface Pointer {
  tab: string;
  chat: string | null;
  kind: "move" | "down" | "up" | "wheel";
  x: number;
  y: number;
  /** `x` and `y` are fractions of the picture (an app's pointer), not page pixels. */
  fraction?: boolean;
}

/** `browser://frame` as an event: the picture is base64. */
interface EventFrame {
  tab: string;
  seq: number;
  data: string;
  width: number;
  height: number;
}

/** `computer://frame`: a picture of a chat's app. */
interface AppFrame {
  surface: string;
  chat: string;
  seq: number;
  data: string;
  width: number;
  height: number;
  title?: string | null;
}

/** `computer://pointer`: the agent's pointer in a chat's app, as fractions of the window. */
interface AppPointer {
  surface: string;
  chat: string;
  x: number;
  y: number;
  phase: string;
}

/** The backend's pointer phases as the cursor's kinds: a press spreads the ring. */
const APP_KINDS: Record<string, Pointer["kind"]> = { move: "move", press: "down", click: "up", drag: "move", wheel: "wheel" };

const titleListeners = new Set<(chat: string, title: string) => void>();

/** Call `listener` with the window title each app frame shows; returns how to stop. */
export function subscribeAppTitles(listener: (chat: string, title: string) => void): () => void {
  start();
  titleListeners.add(listener);
  return () => titleListeners.delete(listener);
}

const lastPointers = new Map<string, Pointer & { at: number }>();
const pointerListeners = new Map<string, Set<(pointer: Pointer) => void>>();
const last = new Map<string, Frame>();
const listeners = new Map<string, Set<(frame: Frame) => void>>();
let started = false;

/** Hand a frame to the canvases of its page (or app). */
function deliver(frame: Frame) {
  last.set(frame.tab, frame);
  listeners.get(frame.tab)?.forEach((listener) => listener(frame));
}

/** A frame that came down the channel. */
function onPacket(buffer: ArrayBuffer) {
  const packet = readPacket(buffer);
  if (!packet) return;
  const { header, image } = packet;
  const app = header.kind === "app";
  const tab = app ? header.surface : header.tab;
  if (typeof tab !== "string") return;
  const size = (value: unknown, fallback: number) => (typeof value === "number" && value > 0 ? value : fallback);
  deliver({ tab, seq: Number(header.seq) || 0, image, width: size(header.width, 1280), height: size(header.height, 800) });
  if (app && typeof header.chat === "string" && typeof header.title === "string" && header.title) {
    const { chat, title } = header as { chat: string; title: string };
    titleListeners.forEach((listener) => listener(chat, title));
  }
}

function start() {
  if (started || !inTauri) return;
  started = true;
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (message) => {
    if (message instanceof ArrayBuffer) onPacket(message);
  };
  void invoke("stage_frames", { onFrame: channel }).catch(() => undefined);
  void listen<Pointer>("browser://pointer", (event) => {
    const pointer = event.payload;
    lastPointers.set(pointer.tab, { ...pointer, at: Date.now() });
    pointerListeners.get(pointer.tab)?.forEach((listener) => listener(pointer));
  }).catch(() => undefined);
  // Until the shell has the channel (or from an older shell), frames come as events.
  void listen<EventFrame>("browser://frame", (event) => {
    const { tab, seq, data, width, height } = event.payload;
    deliver({ tab, seq, image: jpegBlob(data), width, height });
  }).catch(() => undefined);
  void listen<AppFrame>("computer://frame", (event) => {
    const app = event.payload;
    deliver({ tab: app.surface, seq: app.seq, image: jpegBlob(app.data), width: app.width, height: app.height });
    if (app.title) titleListeners.forEach((listener) => listener(app.chat, app.title ?? ""));
  }).catch(() => undefined);
  void listen<AppPointer>("computer://pointer", (event) => {
    const app = event.payload;
    const pointer: Pointer = { tab: app.surface, chat: app.chat, kind: APP_KINDS[app.phase] ?? "move", x: app.x, y: app.y, fraction: true };
    lastPointers.set(pointer.tab, { ...pointer, at: Date.now() });
    pointerListeners.get(pointer.tab)?.forEach((listener) => listener(pointer));
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
