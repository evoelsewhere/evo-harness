import type { Card } from "@/stores/cards-store";
import { isWindows } from "@/lib/platform";
import { BrowserCanvas } from "./browser-canvas";

/**
 * The inside of a card: a page's picture. Without DevTools
 * (not Windows) a page has no live picture: the card says so and offers the panel.
 */
export function CardBody({ card }: { card: Card }) {
  if (!isWindows) {
    return <p className="flex h-full items-center justify-center px-3 text-center text-xs text-muted-foreground">A live view of the page needs Windows; open it in the side panel.</p>;
  }
  return <BrowserCanvas tab={card.id} />;
}
