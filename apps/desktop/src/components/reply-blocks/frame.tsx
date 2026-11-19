// The surface every reply block sits on: title, body, source and note
// (docs/features/rich-replies/spec.md, FR-1, FR-7, FR-8, NFR-3).

import * as React from "react";
import { cn } from "@/lib/utils";

export function BlockFrame({
  title,
  label,
  source,
  note,
  className,
  children,
  ...props
}: {
  title?: string;
  /** The accessible name when there is no title (the component's kind). */
  label: string;
  source?: string;
  note?: string;
  className?: string;
  children: React.ReactNode;
} & Omit<React.ComponentProps<"figure">, "title">) {
  return (
    <figure
      data-slot="reply-block"
      aria-label={title ?? label}
      className={cn(
        "not-prose my-3 min-w-0 rounded-xl bg-card p-4 text-sm ring-1 ring-foreground/8 dark:ring-foreground/10",
        className,
      )}
      {...props}
    >
      {title && <figcaption className="mb-3 text-sm font-medium text-foreground">{title}</figcaption>}
      {children}
      {(source || note) && (
        <div className="mt-3 flex flex-col gap-0.5 text-xs text-muted-foreground">
          {source && <span>Source: {source}</span>}
          {note && <span>{note}</span>}
        </div>
      )}
    </figure>
  );
}

/** A status dot (DESIGN.md: colour only in dots). */
export function Dot({ tone, className }: { tone: "ok" | "warn" | "error" | "info" | "muted"; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        tone === "ok" && "bg-status-ok",
        tone === "warn" && "bg-status-warn",
        tone === "error" && "bg-status-error",
        tone === "info" && "bg-status-info",
        tone === "muted" && "ring-1 ring-muted-foreground/60 ring-inset",
        className,
      )}
    />
  );
}
