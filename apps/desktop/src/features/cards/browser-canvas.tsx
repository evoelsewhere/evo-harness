import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "@/lib/utils";
import { AgentCursor } from "./agent-cursor";
import { keyEvents, mouseParams, toPagePoint, wheelParams } from "./input";
import { noteUserInput } from "./user-input";
import { lastFrame, subscribeFrames, type Frame } from "./stage-stream";

/** DevTools input of one page, one call at a time and in order. */
const queues = new Map<string, Promise<unknown>>();
function dispatch(tab: string, method: string, params: Record<string, unknown>) {
  const next = (queues.get(tab) ?? Promise.resolve()).then(() => invoke("browser_cdp", { tab, method, params })).catch(() => undefined);
  queues.set(tab, next);
}

/**
 * A staged page as a picture: the frames the shell streams, painted on a canvas
 * scaled to fit (the page keeps its desktop layout). With `interactive` the
 * person's mouse, wheel, keys and paste go to the page in its own coordinates.
 * Spec: docs/features/floating-cards (FR-7, FR-8, FR-9).
 */
export function BrowserCanvas({ tab, interactive = true, className }: { tab: string; interactive?: boolean; className?: string }) {
  const canvas = React.useRef<HTMLCanvasElement>(null);
  const size = React.useRef({ w: 1280, h: 800 });
  const [seen, setSeen] = React.useState(() => lastFrame(tab) !== undefined);
  // The canvas element's box and the picture's size, for the virtual cursor.
  const [box, setBox] = React.useState({ width: 0, height: 0 });
  const [pageSize, setPageSize] = React.useState({ w: 1280, h: 800 });

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
      // One picture at a time; a newer frame waits and the ones between are dropped.
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
        setPageSize((now) => (now.w === image.naturalWidth && now.h === image.naturalHeight ? now : { w: image.naturalWidth, h: image.naturalHeight }));
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
    const first = lastFrame(tab);
    if (first) draw(first);
    const stop = subscribeFrames(tab, draw);
    return () => {
      alive = false;
      stop();
    };
  }, [tab]);

  const point = (event: { clientX: number; clientY: number }) => {
    const box = canvas.current?.getBoundingClientRect();
    return box ? toPagePoint(event.clientX, event.clientY, box, { w: size.current.w, h: size.current.h }) : null;
  };

  // The wheel needs a listener that may cancel the page's own scrolling.
  React.useEffect(() => {
    const element = canvas.current;
    if (!element || !interactive) return;
    const onWheel = (event: WheelEvent) => {
      const at = point(event);
      if (!at) return;
      event.preventDefault();
      const box = element.getBoundingClientRect();
      const scale = Math.min(box.width / size.current.w, box.height / size.current.h) || 1;
      dispatch(tab, "Input.dispatchMouseEvent", wheelParams(at, event, 1 / scale));
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [tab, interactive]);

  const mouse = (type: "mousePressed" | "mouseReleased" | "mouseMoved") => (event: React.MouseEvent<HTMLCanvasElement>) => {
    if (!interactive) return;
    const at = point(event);
    if (!at) return;
    if (type !== "mouseMoved") noteUserInput(tab);
    if (type === "mousePressed") event.currentTarget.focus({ preventScroll: true });
    dispatch(tab, "Input.dispatchMouseEvent", mouseParams(type, at, event));
  };

  return (
    <div className={cn("relative flex h-full w-full items-center justify-center overflow-hidden bg-muted/40", className)}>
      <canvas
        ref={canvas}
        width={1280}
        height={800}
        tabIndex={interactive ? 0 : -1}
        aria-label="Page"
        className={cn("h-full w-full object-contain outline-none", !seen && "opacity-0")}
        onMouseDown={mouse("mousePressed")}
        onMouseUp={mouse("mouseReleased")}
        onMouseMove={mouse("mouseMoved")}
        onContextMenu={(event) => interactive && event.preventDefault()}
        onKeyDown={(event) => {
          if (!interactive) return;
          const keys = keyEvents(event);
          if (!keys) return;
          event.preventDefault();
          noteUserInput(tab);
          dispatch(tab, "Input.dispatchKeyEvent", keys.down);
          dispatch(tab, "Input.dispatchKeyEvent", keys.up);
        }}
        onPaste={(event) => {
          if (!interactive) return;
          const text = event.clipboardData.getData("text/plain");
          if (!text) return;
          event.preventDefault();
          dispatch(tab, "Input.insertText", { text });
        }}
      />
      {seen && <AgentCursor tab={tab} box={box} page={pageSize} />}
      {!seen && <p className="absolute text-xs text-muted-foreground">Waiting for the page…</p>}
    </div>
  );
}
