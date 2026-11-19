// A ```mermaid fence in a reply, drawn as a diagram (docs/features/rich-replies/spec.md,
// FR-9, NFR-1, NFR-2). Mermaid loads only with the first diagram and runs
// with `securityLevel: "strict"`: no click handlers, no HTML labels, its
// output sanitised before it is shown.

import * as React from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { BlockFrame } from "./frame";

let renders = 0;

function useDark(): boolean {
  const read = () => document.documentElement.classList.contains("dark");
  const [dark, setDark] = React.useState(read);
  React.useEffect(() => {
    const observer = new MutationObserver(() => setDark(read()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  return dark;
}

export function MermaidBlock({ code, renderSource }: { code: string; renderSource(): React.ReactNode }) {
  const dark = useDark();
  const [state, setState] = React.useState<{ svg?: string; error?: string }>({});
  const [source, setSource] = React.useState(false);

  React.useEffect(() => {
    let live = true;
    setState({});
    void (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: dark ? "dark" : "neutral",
          fontFamily: "Geist Variable, ui-sans-serif, system-ui, sans-serif",
        });
        const { svg } = await mermaid.render(`evo-mermaid-${++renders}`, code);
        if (live) setState({ svg });
      } catch (error) {
        if (live) setState({ error: error instanceof Error ? error.message.split("\n")[0] : "It could not be parsed." });
      }
    })();
    return () => {
      live = false;
    };
  }, [code, dark]);

  if (state.error) {
    return (
      <div className="my-3">
        {renderSource()}
        <p className="mt-1 text-xs text-muted-foreground">Couldn't draw this diagram: {state.error}</p>
      </div>
    );
  }
  return (
    <BlockFrame label="Diagram">
      {source ? (
        renderSource()
      ) : state.svg ? (
        <div
          role="img"
          aria-label="Diagram"
          className="flex justify-center overflow-x-auto [&_svg]:h-auto [&_svg]:max-w-full"
          // Mermaid's strict mode sanitises its SVG (DOMPurify) before it returns it.
          dangerouslySetInnerHTML={{ __html: state.svg }}
        />
      ) : (
        <Skeleton className="h-40 w-full" />
      )}
      <Button variant="ghost" size="xs" className="mt-2 text-muted-foreground" aria-pressed={source} onClick={() => setSource((value) => !value)}>
        {source ? "Show diagram" : "Show source"}
      </Button>
    </BlockFrame>
  );
}
