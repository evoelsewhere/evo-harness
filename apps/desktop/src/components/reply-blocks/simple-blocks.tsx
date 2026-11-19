// The stage-1 components without a chart: stats, bars, steps, callout and
// actions (docs/features/rich-replies/spec.md, FR-15, FR-16, FR-19–FR-22).

import { Button } from "@/components/ui/button";
import { formatValue } from "@/lib/ui-blocks/format";
import type { ActionsBlock, BarsBlock, CalloutBlock, StatsBlock, StepsBlock } from "@/lib/ui-blocks/types";
import { cn } from "@/lib/utils";
import { actionHint, runBlockAction } from "./block-action";
import { Dot } from "./frame";
import { InlineText } from "./inline-text";

export function Stats({ block }: { block: StatsBlock }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(8.5rem,1fr))] gap-x-6 gap-y-4">
      {block.items.map((item, index) => (
        <div key={index} className="flex min-w-0 flex-col gap-1">
          <span className="truncate text-xs text-muted-foreground">{item.label}</span>
          <span className="truncate text-xl font-semibold tracking-tight tabular-nums">
            {formatValue(item.value, item.format)}
          </span>
          {(item.delta !== undefined || item.hint) && (
            <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
              {item.delta !== undefined && (
                <>
                  <Dot tone={item.delta > 0 ? "ok" : item.delta < 0 ? "error" : "muted"} className="size-1.5" />
                  <span className="tabular-nums">
                    {formatValue(item.delta, item.deltaFormat ?? item.format, { signed: true })}
                  </span>
                </>
              )}
              {item.hint && <span className="truncate">{item.hint}</span>}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

export function Bars({ block }: { block: BarsBlock }) {
  const max = block.max ?? Math.max(...block.items.map((item) => Math.abs(item.value)), 0);
  return (
    <div className="flex flex-col gap-1">
      {block.items.map((item, index) => {
        const share = max > 0 ? Math.min(Math.abs(item.value) / max, 1) : 0;
        const row = (
          <>
            <span className="flex items-baseline justify-between gap-3">
              <span className="truncate">{item.label}</span>
              <span className="shrink-0 tabular-nums">{formatValue(item.value, block.format)}</span>
            </span>
            <span className="block h-1.5 w-full overflow-hidden rounded-full bg-muted" aria-hidden>
              <span className="block h-full rounded-full bg-block-viz-1" style={{ width: `${share * 100}%` }} />
            </span>
          </>
        );
        return item.action ? (
          <button
            key={index}
            type="button"
            title={actionHint(item.action)}
            onClick={() => runBlockAction(item.action!)}
            className="-mx-2 flex cursor-pointer flex-col gap-1.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            {row}
          </button>
        ) : (
          <div key={index} className="flex flex-col gap-1.5 py-1.5">
            {row}
          </div>
        );
      })}
    </div>
  );
}

const STEP_TONE = { done: "ok", current: "info", failed: "error", skipped: "muted", todo: "muted" } as const;

export function Steps({ block }: { block: StepsBlock }) {
  return (
    <div role="list" className="flex flex-col gap-2">
      {block.items.map((item, index) => (
        <div role="listitem" key={index} className="flex gap-3">
          <span className="mt-1.5 flex w-4 shrink-0 justify-center">
            <Dot tone={item.status ? STEP_TONE[item.status] : "muted"} />
          </span>
          <span className="flex min-w-0 flex-col">
            <span
              className={cn(
                item.status === "current" && "font-medium",
                item.status === "skipped" && "text-muted-foreground line-through",
              )}
            >
              {item.title}
              {item.status && <span className="sr-only"> ({item.status})</span>}
            </span>
            {item.detail && <span className="text-xs text-muted-foreground">{item.detail}</span>}
          </span>
        </div>
      ))}
    </div>
  );
}

const TONE_DOT = { info: "info", success: "ok", warning: "warn", error: "error" } as const;

export function Callout({ block }: { block: CalloutBlock }) {
  return (
    <div className="flex gap-3" role={block.tone === "error" || block.tone === "warning" ? "note" : undefined}>
      <Dot tone={TONE_DOT[block.tone]} className="mt-1.5" />
      <span className="sr-only">{block.tone}: </span>
      <span className="min-w-0 leading-6">
        <InlineText text={block.text} />
      </span>
    </div>
  );
}

export function Actions({ block }: { block: ActionsBlock }) {
  return (
    <div className="flex flex-wrap gap-2">
      {block.items.map((item, index) => (
        <Button
          key={index}
          size="sm"
          variant={item.primary ? "default" : "outline"}
          title={actionHint(item.action)}
          onClick={() => runBlockAction(item.action)}
        >
          {item.label}
        </Button>
      ))}
    </div>
  );
}
