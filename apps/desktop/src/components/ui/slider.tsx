import { Slider as SliderPrimitive } from "@base-ui/react/slider"
import { cn } from "@/lib/utils"

/** A single-value slider: the filled track and a round thumb in the accent colour. */
function Slider({ className, ...props }: SliderPrimitive.Root.Props<number>) {
  return (
    <SliderPrimitive.Root data-slot="slider" className={cn("w-full", className)} {...props}>
      <SliderPrimitive.Control className="relative flex h-5 w-full touch-none items-center select-none data-disabled:opacity-50">
        <SliderPrimitive.Track className="relative h-1.5 w-full rounded-full bg-muted shadow-hairline">
          <SliderPrimitive.Indicator className="h-full rounded-full bg-brand" />
          <SliderPrimitive.Thumb className="size-4 rounded-full bg-background shadow-[0_0_0_1px_var(--hairline),0_1px_3px_#0000002e] outline-none after:absolute after:inset-1 after:rounded-full after:bg-brand focus-visible:shadow-[0_0_0_2px_var(--background),0_0_0_4px_var(--ring)] data-dragging:scale-110 data-disabled:pointer-events-none" />
        </SliderPrimitive.Track>
      </SliderPrimitive.Control>
    </SliderPrimitive.Root>
  )
}

export { Slider }
