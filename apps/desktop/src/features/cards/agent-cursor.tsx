import * as React from "react";
import { lastPointer, subscribePointer, type Pointer } from "./stage-stream";
import { toDisplayPoint } from "./input";

/** How long the cursor stays after the agent's last mouse event. */
const LINGER_MS = 4000;

/**
 * The agent's virtual cursor over a page's picture (docs/features/floating-cards,
 * FR-17): an arrow that glides to where an agent tool puts the mouse and a ring that
 * spreads where it clicks, so you see what the agent is pointing at. It is drawn over
 * the picture only, never in the page, and fades after a few seconds of quiet.
 * `box` is the canvas element's size, `page` the picture's size in page pixels.
 */
export function AgentCursor({ tab, box, page }: { tab: string; box: { width: number; height: number }; page: { w: number; h: number } }) {
  const [pointer, setPointer] = React.useState<(Pointer & { at: number }) | null>(() => {
    const recent = lastPointer(tab);
    return recent && Date.now() - recent.at < LINGER_MS ? recent : null;
  });
  // Each press restarts the ring.
  const [clicks, setClicks] = React.useState(0);
  const [visible, setVisible] = React.useState(pointer !== null);

  React.useEffect(
    () =>
      subscribePointer(tab, (next) => {
        setPointer({ ...next, at: Date.now() });
        setVisible(true);
        if (next.kind === "down") setClicks((n) => n + 1);
      }),
    [tab],
  );

  React.useEffect(() => {
    if (!visible || !pointer) return;
    const timer = setTimeout(() => setVisible(false), Math.max(LINGER_MS - (Date.now() - pointer.at), 0));
    return () => clearTimeout(timer);
  }, [visible, pointer]);

  if (!pointer || box.width <= 0) return null;
  const at = toDisplayPoint(pointer.x, pointer.y, box, page);
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 overflow-hidden" data-slot="agent-cursor">
      <div
        className="absolute top-0 left-0 transition-[transform,opacity] duration-150 ease-out"
        style={{ transform: `translate(${at.x}px, ${at.y}px)`, opacity: visible ? 1 : 0 }}
      >
        {clicks > 0 && (
          <span
            key={clicks}
            className="absolute top-0 left-0 size-4 rounded-full border-[1.5px] border-[#9aa0ab]"
            style={{ animation: "agent-cursor-ripple 450ms ease-out forwards" }}
          />
        )}
        <svg width="11" height="11" viewBox="0 0 17 17" className="relative" style={{ marginLeft: -1.5, marginTop: -1.5, filter: "drop-shadow(0 0 2px rgba(96, 165, 250, 0.65))" }}>
          {/* A small, symmetric arrowhead with softly rounded corners: a light blue outline under a dark shape, both with round joins, and a faint blue glow. */}
          <path d="M2.8 2.8 L16.02 7.25 L8.6 8.6 L7.25 16.02 Z" fill="#9ccbff" stroke="#9ccbff" strokeWidth="3.4" strokeLinejoin="round" />
          <path d="M2.8 2.8 L16.02 7.25 L8.6 8.6 L7.25 16.02 Z" fill="#1c2033" stroke="#1c2033" strokeWidth="1.8" strokeLinejoin="round" />
        </svg>
      </div>
    </div>
  );
}
