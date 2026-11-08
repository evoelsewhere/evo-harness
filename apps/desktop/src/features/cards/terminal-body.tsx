import * as React from "react";
import { terminalInstance } from "@/stores/terminal-store";

/**
 * A shell's live output in a card: the tab's own xterm element moved in (nothing is
 * restarted, the scrollback comes with it) and fitted to the card. View-only: the
 * card takes no keys or clicks, typing happens in the side panel (FR-7, FR-8).
 */
export function TerminalBody({ tab }: { tab: string }) {
  const container = React.useRef<HTMLDivElement>(null);
  const instance = terminalInstance(tab);

  React.useLayoutEffect(() => {
    const element = container.current;
    if (!element || !instance) return;
    void instance.attach(element);
    const observer = new ResizeObserver(() => instance.scheduleFit());
    observer.observe(element);
    return () => {
      observer.disconnect();
      instance.detach();
    };
  }, [instance]);

  if (!instance) return <p className="flex h-full items-center justify-center text-xs text-muted-foreground">The shell is gone.</p>;
  return <div ref={container} aria-label="Terminal output (view only)" className="pointer-events-none h-full w-full overflow-hidden p-1 select-none" />;
}
