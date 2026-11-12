"use client"

import { Checkbox as CheckboxPrimitive } from "@base-ui/react/checkbox"
import { CheckIcon } from "lucide-react"
import { cn } from "@/lib/utils"

/** A checkbox; the box takes the accent colour when checked, like the switch. */
function Checkbox({ className, ...props }: CheckboxPrimitive.Root.Props) {
  return (
    <CheckboxPrimitive.Root
      data-slot="checkbox"
      className={cn(
        "peer flex size-4 shrink-0 cursor-default items-center justify-center rounded-[5px] shadow-hairline transition-colors outline-none data-checked:bg-brand data-checked:text-brand-foreground data-checked:shadow-none data-unchecked:bg-background data-disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-ring/50",
        className
      )}
      {...props}
    >
      <CheckboxPrimitive.Indicator data-slot="checkbox-indicator" className="flex items-center justify-center text-current">
        <CheckIcon className="size-3" strokeWidth={3} />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  )
}

export { Checkbox }
