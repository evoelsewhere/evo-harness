// Pinned messages (docs/features/evo-mates-messaging/spec.md, FR-14): a bar
// under the header showing one pin at a time; the arrows step through them,
// a click jumps to the message, ✕ unpins it.

import * as React from "react";
import { ChevronDownIcon, ChevronUpIcon, PinIcon, XIcon } from "lucide-react";
import { useMatesPrefs } from "@/stores/mates-prefs-store";

const EMPTY: never[] = [];

/** Scroll to the message whose element carries `data-key` or `data-entry` = `key`, and flash it. */
export function jumpToMessage(key: string): boolean {
  const el =
    document.querySelector(`[data-key="${CSS.escape(key)}"] [data-bubble]`) ??
    document.querySelector(`[data-entry="${CSS.escape(key)}"] [data-bubble]`) ??
    document.querySelector(`[data-entry="${CSS.escape(key)}"]`);
  if (!el) return false;
  el.scrollIntoView({ block: "center", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  el.classList.remove("mates-flash");
  void (el as HTMLElement).offsetWidth;
  el.classList.add("mates-flash");
  window.setTimeout(() => el.classList.remove("mates-flash"), 1600);
  return true;
}

export function PinsBar({ chat }: { chat: string }) {
  const pins = useMatesPrefs((s) => s.pins[chat] ?? EMPTY);
  const unpin = useMatesPrefs((s) => s.unpin);
  const [index, setIndex] = React.useState(0);
  React.useEffect(() => setIndex(Math.max(0, pins.length - 1)), [pins.length]);
  if (!pins.length) return null;
  const at = Math.min(index, pins.length - 1);
  const pin = pins[at];
  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border/50 bg-background/80 px-4 backdrop-blur-sm" role="region" aria-label="Pinned messages">
      <span aria-hidden className="h-6 w-0.5 rounded-full bg-brand" />
      <PinIcon className="size-3.5 shrink-0 text-brand" />
      <button
        type="button"
        onClick={() => jumpToMessage(pin.key)}
        className="flex min-w-0 flex-1 flex-col items-start text-left"
        title="Go to the message"
      >
        <span className="text-[11px] font-medium text-brand">
          Pinned{pins.length > 1 ? ` · ${at + 1} of ${pins.length}` : ""}
        </span>
        <span className="w-full truncate text-xs text-foreground">{pin.excerpt}</span>
      </button>
      {pins.length > 1 && (
        <span className="flex shrink-0">
          <button
            type="button"
            aria-label="Previous pin"
            onClick={() => setIndex((i) => (i - 1 + pins.length) % pins.length)}
            className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <ChevronUpIcon className="size-3.5" />
          </button>
          <button
            type="button"
            aria-label="Next pin"
            onClick={() => setIndex((i) => (i + 1) % pins.length)}
            className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <ChevronDownIcon className="size-3.5" />
          </button>
        </span>
      )}
      <button
        type="button"
        aria-label="Unpin"
        onClick={() => unpin(chat, pin.key)}
        className="shrink-0 rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <XIcon className="size-3.5" />
      </button>
    </div>
  );
}
