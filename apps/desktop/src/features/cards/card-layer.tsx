import * as React from "react";
import "@/stores/cards-sync";
import { useEngineStore } from "@/stores/engine-store";
import { Card } from "./card";
import { CardBody } from "./card-body";
import { useChatCards } from "./use-cards";

/**
 * The layer over the chat's column where the showing chat's cards float. It
 * ignores the pointer except on the cards themselves, sits under menus and
 * dialogs (which are portalled above it), and never takes the keyboard focus.
 * Spec: docs/features/floating-cards (FR-1 – FR-3, FR-5, FR-11, FR-12).
 */
export function CardLayer() {
  const chat = useEngineStore((s) => s.activeId);
  const { shown } = useChatCards(chat);
  const ref = React.useRef<HTMLDivElement>(null);
  const [layer, setLayer] = React.useState({ w: 0, h: 0 });

  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => setLayer((now) => (now.w === element.clientWidth && now.h === element.clientHeight ? now : { w: element.clientWidth, h: element.clientHeight }));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={ref} data-slot="agent-card-layer" className="pointer-events-none absolute inset-0 z-30 overflow-hidden">
      {chat &&
        layer.w > 0 &&
        shown.map((card, index) => (
          <Card key={card.key} chat={chat} card={card} slot={index} layer={layer}>
            <CardBody card={card} />
          </Card>
        ))}
    </div>
  );
}
