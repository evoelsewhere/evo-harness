import type { Card } from "@/stores/cards-store";
import { useBrowserStore } from "@/stores/browser-store";
import { useTerminalStore } from "@/stores/terminal-store";

/**
 * The inside of a card. Until the live views land (browser canvas, terminal
 * output) it says which tab the card is for.
 */
export function CardBody({ card }: { card: Card }) {
  const url = useBrowserStore((s) => (card.kind === "browser" ? s.all.find((t) => t.id === card.id)?.url : undefined));
  const cwd = useTerminalStore((s) => (card.kind === "terminal" ? s.tabs.find((t) => t.key === card.id)?.cwd : undefined));
  return <div className="flex h-full items-center justify-center px-3 text-center text-xs break-all text-muted-foreground">{card.kind === "browser" ? (url ?? "") : (cwd ?? "")}</div>;
}
