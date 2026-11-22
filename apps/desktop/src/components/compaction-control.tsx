// The auto-compact controls (docs/features/auto-compact-threshold/spec.md, FR-4,
// FR-5, NFR-1, NFR-2): a switch and a slider for the share of the model's
// window at which a chat compacts. Used in Settings and in the context panel;
// both read and write the same store.

import * as React from "react";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { formatTokens } from "@/lib/format";
import {
  MAX_TRIGGER_PERCENT,
  MIN_TRIGGER_PERCENT,
  clampPercent,
  useCompactionStore,
} from "@/stores/compaction-store";

/** The saved setting, read once, and a way to change it. */
export function useCompaction() {
  const settings = useCompactionStore((s) => s.settings);
  const save = useCompactionStore((s) => s.save);
  const load = useCompactionStore((s) => s.load);
  React.useEffect(() => {
    void load();
  }, [load]);
  return { settings, save };
}

/** Auto-compact on or off. */
export function CompactionSwitch({ labelledBy }: { labelledBy: string }) {
  const { settings, save } = useCompaction();
  return (
    <Switch
      aria-labelledby={labelledBy}
      checked={settings.enabled}
      onCheckedChange={(enabled) => void save({ enabled })}
    />
  );
}

/** How long a drag waits for a pause before the value is saved. */
const SAVE_DELAY = 250;

/**
 * The trigger percent. While it is dragged only this control changes; the
 * value is saved when the thumb is let go, on a key press, or once the pointer
 * pauses (so a panel that closes mid-drag still keeps it), not on every pixel.
 * `window` (tokens) adds what the percent is for the model of the chat.
 */
export function CompactionSlider({ window, label = "Auto-compact at" }: { window?: number; label?: string }) {
  const { settings, save } = useCompaction();
  const [draft, setDraft] = React.useState<number | null>(null);
  const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const pending = React.useRef<number | null>(null);

  const flush = React.useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (pending.current !== null) void save({ triggerPercent: pending.current });
    pending.current = null;
  }, [save]);
  // Nothing is lost when the control goes away mid-drag.
  React.useEffect(() => flush, [flush]);

  const shown = draft ?? settings.triggerPercent;
  return (
    <div className={settings.enabled ? undefined : "opacity-60"}>
      <div className="flex items-baseline justify-between gap-3 text-[13px]">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-mono tabular-nums">
          {shown}%
          {window ? <span className="text-muted-foreground"> · {formatTokens((window * shown) / 100)}</span> : null}
        </span>
      </div>
      <Slider
        aria-label={label}
        min={MIN_TRIGGER_PERCENT}
        max={MAX_TRIGGER_PERCENT}
        step={1}
        value={shown}
        disabled={!settings.enabled}
        onValueChange={(value) => {
          const next = clampPercent(value);
          setDraft(next);
          pending.current = next;
          if (timer.current) clearTimeout(timer.current);
          timer.current = setTimeout(flush, SAVE_DELAY);
        }}
        onValueCommitted={(value) => {
          pending.current = clampPercent(value);
          flush();
          setDraft(null);
        }}
      />
    </div>
  );
}
