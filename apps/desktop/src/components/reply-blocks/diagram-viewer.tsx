// A diagram opened full screen (docs/features/rich-replies/spec.md, FR-9):
// fitted to the window at first, then zoomed with the buttons, Ctrl + wheel
// or + and -, and moved by dragging or scrolling.

import * as React from "react";
import { MinusIcon, PlusIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

const MIN = 0.1;
const MAX = 8;
const STEP = 1.25;
const clamp = (scale: number) => Math.min(MAX, Math.max(MIN, scale));

/** The SVG's own size, from its viewBox. */
function naturalSize(svg: SVGSVGElement): { width: number; height: number } | undefined {
  const box = svg.viewBox?.baseVal;
  if (box && box.width > 0 && box.height > 0) return { width: box.width, height: box.height };
  const rect = svg.getBoundingClientRect();
  return rect.width > 0 ? { width: rect.width, height: rect.height } : undefined;
}

function Viewer({ svg }: { svg: string }) {
  const viewport = React.useRef<HTMLDivElement>(null);
  const content = React.useRef<HTMLDivElement>(null);
  const size = React.useRef<{ width: number; height: number }>(undefined);
  const [scale, setScale] = React.useState<number>();
  const [fit, setFit] = React.useState<number>();
  const drag = React.useRef<{ x: number; y: number; left: number; top: number }>(undefined);

  // Measure once the SVG is in place, and fit it to the window.
  React.useLayoutEffect(() => {
    const element = content.current?.querySelector("svg");
    const box = viewport.current;
    if (!element || !box) return;
    size.current = naturalSize(element);
    if (!size.current) return;
    // Small diagrams grow at most to twice their size.
    const fitted = clamp(Math.min(2, (box.clientWidth - 48) / size.current.width, (box.clientHeight - 48) / size.current.height));
    setFit(fitted);
    setScale(fitted);
  }, [svg]);

  React.useLayoutEffect(() => {
    const element = content.current?.querySelector("svg");
    if (!element || !size.current || scale === undefined) return;
    element.style.maxWidth = "none";
    element.style.width = `${size.current.width * scale}px`;
    element.style.height = `${size.current.height * scale}px`;
  }, [scale, svg]);

  /** Zooms by `factor`, keeping the point under (x, y) of the viewport still. */
  const zoom = React.useCallback((factor: number, x?: number, y?: number) => {
    const box = viewport.current;
    setScale((current) => {
      if (current === undefined || !box) return current;
      const next = clamp(current * factor);
      const px = x ?? box.clientWidth / 2;
      const py = y ?? box.clientHeight / 2;
      const left = box.scrollLeft;
      const top = box.scrollTop;
      requestAnimationFrame(() => {
        box.scrollLeft = ((left + px) * next) / current - px;
        box.scrollTop = ((top + py) * next) / current - py;
      });
      return next;
    });
  }, []);

  // Ctrl + wheel zooms; React's wheel listener is passive, so attach it here.
  React.useEffect(() => {
    const box = viewport.current;
    if (!box) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      const rect = box.getBoundingClientRect();
      zoom(event.deltaY < 0 ? 1.1 : 1 / 1.1, event.clientX - rect.left, event.clientY - rect.top);
    };
    box.addEventListener("wheel", onWheel, { passive: false });
    return () => box.removeEventListener("wheel", onWheel);
  }, [zoom]);

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "+" || event.key === "=") zoom(STEP);
    else if (event.key === "-") zoom(1 / STEP);
    else if (event.key === "0" && fit !== undefined) setScale(fit);
    else return;
    event.preventDefault();
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex items-center gap-1 pr-10">
        <DialogTitle className="mr-auto text-sm font-medium">Diagram</DialogTitle>
        <Button variant="ghost" size="icon-sm" aria-label="Zoom out" onClick={() => zoom(1 / STEP)}>
          <MinusIcon />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="min-w-16 tabular-nums"
          aria-label="Fit to window"
          title="Fit to window"
          onClick={() => fit !== undefined && setScale(fit)}
        >
          {scale === undefined ? "—" : `${Math.round(scale * 100)}%`}
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Zoom in" onClick={() => zoom(STEP)}>
          <PlusIcon />
        </Button>
      </div>
      <div
        ref={viewport}
        role="img"
        aria-label="Diagram"
        tabIndex={0}
        onKeyDown={onKeyDown}
        className="min-h-0 flex-1 cursor-grab overflow-auto rounded-lg bg-card ring-1 ring-foreground/8 outline-none select-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing dark:ring-foreground/10"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          const box = event.currentTarget;
          drag.current = { x: event.clientX, y: event.clientY, left: box.scrollLeft, top: box.scrollTop };
          box.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const start = drag.current;
          if (!start) return;
          event.currentTarget.scrollLeft = start.left - (event.clientX - start.x);
          event.currentTarget.scrollTop = start.top - (event.clientY - start.y);
        }}
        onPointerUp={() => (drag.current = undefined)}
        onPointerCancel={() => (drag.current = undefined)}
      >
        <div
          ref={content}
          className="flex min-h-full min-w-fit items-center justify-center p-6"
          // Mermaid's strict mode sanitises its SVG (DOMPurify) before it returns it.
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      </div>
    </div>
  );
}

/** The diagram in a dialog that fills the window. */
export function DiagramViewer({ svg, open, onOpenChange }: { svg: string; open: boolean; onOpenChange(open: boolean): void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[calc(100vh-2rem)] w-[calc(100vw-2rem)] max-w-none flex-col p-4 sm:max-w-none">
        {open && <Viewer svg={svg} />}
      </DialogContent>
    </Dialog>
  );
}
