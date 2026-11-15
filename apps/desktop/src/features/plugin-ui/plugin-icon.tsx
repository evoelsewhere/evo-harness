// A plugin view's icon as a component (docs/features/plugin-ui/spec.md, FR-4 – FR-6).

import type * as React from "react";
import { PuzzleIcon } from "lucide-react";
import { cn } from "@/lib/utils";

type IconComponent = React.ComponentType<{ className?: string }>;
const icons = new Map<string, IconComponent>();

/** A component drawing a view's icon (its data URL, else a puzzle piece); stable per icon. */
export function pluginIcon(icon?: string): IconComponent {
  const key = icon ?? "";
  let component = icons.get(key);
  if (!component) {
    component = icon
      ? ({ className }) => <img src={icon} alt="" aria-hidden className={cn("size-4 shrink-0 object-contain", className)} />
      : ({ className }) => <PuzzleIcon className={className} />;
    icons.set(key, component);
  }
  return component;
}
