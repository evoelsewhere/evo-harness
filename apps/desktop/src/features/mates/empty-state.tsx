// One empty state for every Evo Mates list (docs/features/evo-mates-ui-v2/
// spec.md, FR-29): one sentence and, where there is one, one action.

import * as React from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function EmptyState({
  icon,
  title,
  text,
  action,
  onAction,
  compact = false,
  className,
}: {
  icon?: React.ReactNode;
  title?: string;
  text: string;
  action?: string;
  onAction?(): void;
  /** A line in a list (the sidebar) instead of a centred block. */
  compact?: boolean;
  className?: string;
}) {
  if (compact)
    return (
      <div className={cn("flex items-center gap-2 px-2.5 py-1 text-[13px] text-muted-foreground", className)}>
        <span className="min-w-0 flex-1">{text}</span>
        {action && onAction && (
          <button type="button" onClick={onAction} className="no-drag shrink-0 rounded-md px-1 text-[12px] font-medium text-brand hover:underline">
            {action}
          </button>
        )}
      </div>
    );
  return (
    <div className={cn("flex flex-col items-center gap-2 px-6 py-8 text-center", className)}>
      {icon && <div className="mb-1 flex size-11 items-center justify-center rounded-2xl bg-muted text-muted-foreground [&_svg]:size-5">{icon}</div>}
      {title && <div className="text-[14px] font-medium text-foreground">{title}</div>}
      <p className="max-w-xs text-[13px] text-muted-foreground">{text}</p>
      {action && onAction && (
        <Button size="sm" variant="secondary" className="mt-1 rounded-full px-3.5" onClick={onAction}>
          {action}
        </Button>
      )}
    </div>
  );
}
