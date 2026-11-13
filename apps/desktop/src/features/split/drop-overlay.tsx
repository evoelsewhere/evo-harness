import * as React from "react";
import { createPortal } from "react-dom";
import { MessageSquareIcon } from "lucide-react";
import { isNarrow } from "@/stores/split-rules";
import { useSplitStore } from "@/stores/split-store";
import { UNTITLED, useSessionStore } from "@/stores/session-store";
import { useDragStore } from "./drag";
import { previewRect } from "./drop-geometry";
import { SPRING, SPRING_MS } from "./motion";

/**
 * Where the dragged chat or pane will land: a translucent box over that part
 * of the chat area that glides between places as the pointer moves
 * (FR-2, FR-9). `width`/`height` are the area's size.
 */
export function DropPreview({ width, height }: { width: number; height: number }) {
  const target = useDragStore((s) => s.target);
  const dragging = useDragStore((s) => s.payload !== null);
  const { panes, focus, ratio } = useSplitStore();
  // The last place stays while the box fades, so it does not jump on leaving.
  const last = React.useRef<{ left: number; top: number; width: number; height: number } | null>(null);
  if (target) {
    last.current = previewRect({ left: 0, top: 0, width, height }, target, { panes, focus, ratio, narrow: isNarrow(width) });
  }
  if (!dragging) last.current = null;
  const rect = last.current;
  const shown = Boolean(target);
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 z-30">
      <div
        className="absolute rounded-xl bg-brand/8 ring-2 ring-brand/50 ring-inset motion-reduce:transition-[opacity]"
        style={{
          left: (rect?.left ?? 0) + 6,
          top: (rect?.top ?? 0) + 6,
          width: Math.max(0, (rect?.width ?? 0) - 12),
          height: Math.max(0, (rect?.height ?? 0) - 12),
          opacity: shown ? 1 : 0,
          transform: shown ? "none" : "scale(.98)",
          transitionProperty: "left, top, width, height, opacity, transform",
          transitionDuration: `${SPRING_MS}ms, ${SPRING_MS}ms, ${SPRING_MS}ms, ${SPRING_MS}ms, 160ms, ${SPRING_MS}ms`,
          transitionTimingFunction: SPRING,
        }}
      />
    </div>
  );
}

/** The dragged chat's title, following the pointer. */
export function DragGhost() {
  const payload = useDragStore((s) => s.payload);
  const x = useDragStore((s) => s.x);
  const y = useDragStore((s) => s.y);
  const panes = useSplitStore((s) => s.panes);
  const chat = payload?.kind === "chat" ? payload.chat : payload && panes ? panes[payload.index] : null;
  const title = useSessionStore((s) => (chat ? s.sessions.find((c) => c.id === chat)?.title : undefined)) || UNTITLED;
  if (!payload || !chat) return null;
  return createPortal(
    <div
      aria-hidden
      className="pointer-events-none fixed top-0 left-0 z-[100] flex max-w-60 items-center gap-1.5 rounded-lg bg-popover px-2.5 py-1.5 text-xs font-medium text-popover-foreground shadow-lg ring-1 ring-border animate-in fade-in-0 zoom-in-95"
      style={{ transform: `translate(${x + 12}px, ${y + 10}px)` }}
    >
      <MessageSquareIcon className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate">{title}</span>
    </div>,
    document.body,
  );
}
