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
            className="absolute top-0 left-0 size-7 rounded-full border-[3px] border-[#9aa0ab]"
            style={{ animation: "agent-cursor-ripple 450ms ease-out forwards" }}
          />
        )}
        <svg width="22" height="26" viewBox="0 0 22 26" className="relative drop-shadow-sm" style={{ marginLeft: -1, marginTop: -1 }}>
          <path d="M1.5 1.5 L20 9.5 Q21.4 10.2 20.2 11.2 L13.6 14.6 L9.4 24 Q8.7 25.4 8 24 Z" fill="#1c2033" stroke="#ffffff" strokeWidth="1.4" strokeLinejoin="round" />
        </svg>
      </div>
    </div>
  );
}
