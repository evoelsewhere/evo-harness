import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "@/lib/utils";
import { useComputerStore } from "@/stores/computer-store";
import { AgentCursor } from "./agent-cursor";
import { appKeyInput, toFraction } from "./app-input";
import { toPagePoint } from "./input";
import { lastFrame, subscribeAppTitles, subscribeFrames, type Frame } from "./stage-stream";

/** The person's input to one chat's app, one at a time and in order. */
const queues = new Map<string, Promise<unknown>>();
function send(chat: string, input: Record<string, unknown>) {
  const next = (queues.get(chat) ?? Promise.resolve()).then(() => invoke("computer_input", { chat, input })).catch(() => undefined);
  queues.set(chat, next);
}

/** Frames carry the window's title; the card's bar shows the newest. */
let titlesWired = false;
function wireTitles() {
  if (titlesWired) return;
  titlesWired = true;
  subscribeAppTitles((chat, title) => useComputerStore.getState().setTitle(chat, title));
}

/**
 * The app a chat's agent controls, as a picture: the frames the shell streams
 * (`computer://frame`), scaled to fit, with the agent's virtual cursor over
 * them. The person's clicks, wheel and keys go to the app through the same
 * background channels the agent uses, so the window stays off their screen.
 * Spec: docs/features/computer-app-control (FR-10, FR-11, FR-13).
 */
export function AppCanvas({ chat, className }: { chat: string; className?: string }) {
  const surface = `app:${chat}`;
  const canvas = React.useRef<HTMLCanvasElement>(null);
  const size = React.useRef({ w: 1280, h: 800 });
  const [seen, setSeen] = React.useState(() => lastFrame(surface) !== undefined);
  const [box, setBox] = React.useState({ width: 0, height: 0 });
  const [picture, setPicture] = React.useState({ w: 1280, h: 800 });
  const stopped = useComputerStore((s) => s.apps[chat]?.state === "stopped");

  React.useEffect(wireTitles, []);

  React.useLayoutEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const measure = () => setBox({ width: element.clientWidth, height: element.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  React.useEffect(() => {
    const element = canvas.current;
    const context = element?.getContext("2d");
    if (!element || !context) return;
    let alive = true;
    let loading = false;
    let pending: Frame | null = null;
    const draw = (frame: Frame) => {
      if (loading) {
        pending = frame;
        return;
      }
      loading = true;
      const image = new Image();
      image.onload = () => {
        loading = false;
        if (!alive) return;
        if (element.width !== image.naturalWidth || element.height !== image.naturalHeight) {
          element.width = image.naturalWidth;
          element.height = image.naturalHeight;
        }
        size.current = { w: image.naturalWidth, h: image.naturalHeight };
        setPicture((now) => (now.w === image.naturalWidth && now.h === image.naturalHeight ? now : { w: image.naturalWidth, h: image.naturalHeight }));
        context.drawImage(image, 0, 0);
        setSeen(true);
        if (pending) {
          const next = pending;
          pending = null;
          draw(next);
        }
      };
      image.onerror = () => {
        loading = false;
      };
      image.src = `data:image/jpeg;base64,${frame.data}`;
    };
    const first = lastFrame(surface);
    if (first) draw(first);
    const stop = subscribeFrames(surface, draw);
    return () => {
      alive = false;
      stop();
    };
  }, [surface]);

  /** Where an event is on the app's window, as fractions of it. */
  const at = (event: { clientX: number; clientY: number }) => {
    const rect = canvas.current?.getBoundingClientRect();
    return rect ? toFraction(toPagePoint(event.clientX, event.clientY, rect, size.current), size.current) : null;
  };

  // The wheel needs a listener that may cancel the chat's own scrolling.
  React.useEffect(() => {
    const element = canvas.current;
    if (!element || stopped) return;
    let quiet = 0;
    const onWheel = (event: WheelEvent) => {
      const point = at(event);
      if (!point) return;
      event.preventDefault();
      // One notch per burst slice: a trackpad sends dozens of tiny events.
      const now = Date.now();
      if (now - quiet < 60) return;
      quiet = now;
      send(chat, { kind: "scroll", ...point, direction: event.deltaY < 0 ? "up" : "down" });
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [chat, stopped]);

  return (
    <div className={cn("relative flex h-full w-full items-center justify-center overflow-hidden bg-muted/40", className)}>
      <canvas
        ref={canvas}
        width={1280}
        height={800}
        tabIndex={stopped ? -1 : 0}
        aria-label="App"
        className={cn("h-full w-full object-contain outline-none", !seen && "opacity-0")}
        onMouseDown={(event) => event.currentTarget.focus({ preventScroll: true })}
        onMouseUp={(event) => {
          if (stopped) return;
          const point = at(event);
          if (!point) return;
          send(chat, { kind: "click", ...point, button: event.button === 2 ? "right" : event.button === 1 ? "middle" : "left", clicks: 1 });
        }}
        onContextMenu={(event) => event.preventDefault()}
        onKeyDown={(event) => {
          if (stopped) return;
          const input = appKeyInput(event);
          if (!input) return;
          event.preventDefault();
          send(chat, input);
        }}
      />
      {seen && <AgentCursor tab={surface} box={box} page={picture} />}
      {!seen && <p className="absolute text-xs text-muted-foreground">Waiting for the app…</p>}
      {stopped && (
        <p className="absolute inset-x-0 bottom-0 bg-background/90 px-3 py-2 text-center text-xs text-muted-foreground">
          You stopped app control. The agent cannot act in an app until you allow it again.
        </p>
      )}
    </div>
  );
}
