// A ```mermaid fence in a reply, drawn as a diagram (docs/features/rich-replies/spec.md,
// FR-9, NFR-1, NFR-2). Mermaid loads only with the first diagram and runs
// with `securityLevel: "strict"`: no click handlers, no HTML labels, its
// output sanitised before it is shown.

import * as React from "react";
import { Maximize2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { DiagramViewer } from "./diagram-viewer";
import { BlockFrame } from "./frame";

let renders = 0;

/**
 * What the diagram's colours depend on: light or dark, and the background
 * picked in Settings → Appearance (`data-bg-*` and `--bg-*` on the root).
 */
function useThemeKey(): string {
  const read = () => {
    const root = document.documentElement;
    return [root.className, root.getAttribute("data-bg-light"), root.getAttribute("data-bg-dark"), root.style.cssText].join("|");
  };
  const [key, setKey] = React.useState(read);
  React.useEffect(() => {
    const observer = new MutationObserver(() => setKey(read()));
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style", "data-bg-light", "data-bg-dark"],
    });
    return () => observer.disconnect();
  }, []);
  return key;
}

/**
 * The app's colour tokens as hex, for Mermaid: it mixes colours itself and
 * reads only hex, rgb and hsl, while the tokens may be `color-mix()` of the
 * chosen background. A canvas pixel gives each one's sRGB value.
 */
function themeColors(): Record<string, string> {
  const probe = document.createElement("span");
  probe.style.display = "none";
  document.body.append(probe);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  const hex = (css: string) => {
    probe.style.color = "";
    probe.style.color = css;
    const color = getComputedStyle(probe).color;
    if (!context) return color;
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = "#000";
    context.fillStyle = color;
    context.fillRect(0, 0, 1, 1);
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
    return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
  };
  const card = "var(--card)";
  const colors = {
    card: hex(card),
    fill: hex("var(--secondary)"),
    muted: hex("var(--muted)"),
    note: hex("var(--accent)"),
    text: hex("var(--foreground)"),
    line: hex("var(--muted-foreground)"),
    border: hex(`color-mix(in oklab, var(--foreground) 22%, ${card})`),
  };
  probe.remove();
  return colors;
}

/** Mermaid's `base` theme in the app's colours (DESIGN.md: neutral surfaces). */
function themeVariables(dark: boolean) {
  const c = themeColors();
  return {
    darkMode: dark,
    background: c.card,
    fontFamily: getComputedStyle(document.body).fontFamily,
    primaryColor: c.fill,
    primaryTextColor: c.text,
    primaryBorderColor: c.border,
    secondaryColor: c.muted,
    tertiaryColor: c.card,
    lineColor: c.line,
    textColor: c.text,
    mainBkg: c.fill,
    nodeBorder: c.border,
    clusterBkg: c.muted,
    clusterBorder: c.border,
    titleColor: c.text,
    edgeLabelBackground: c.card,
    noteBkgColor: c.note,
    noteTextColor: c.text,
    noteBorderColor: c.border,
    actorBkg: c.fill,
    actorBorder: c.border,
    actorTextColor: c.text,
    actorLineColor: c.line,
    signalColor: c.line,
    signalTextColor: c.text,
    labelBoxBkgColor: c.fill,
    labelBoxBorderColor: c.border,
    labelTextColor: c.text,
    loopTextColor: c.text,
    activationBkgColor: c.muted,
    activationBorderColor: c.border,
    sequenceNumberColor: c.card,
  };
}

export function MermaidBlock({ code, renderSource }: { code: string; renderSource(): React.ReactNode }) {
  const theme = useThemeKey();
  const [state, setState] = React.useState<{ svg?: string; id?: string; error?: string }>({});
  const [source, setSource] = React.useState(false);
  const [full, setFull] = React.useState(false);
  // The full-screen copy gets its own ids: Mermaid scopes styles and arrow
  // markers by the SVG's id, and two copies with one id would share them.
  const fullSvg = React.useMemo(
    () => (state.svg && state.id ? state.svg.replaceAll(state.id, `${state.id}-full`) : undefined),
    [state.svg, state.id],
  );

  React.useEffect(() => {
    let live = true;
    setState({});
    void (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: "base",
          themeVariables: themeVariables(document.documentElement.classList.contains("dark")),
        });
        const id = `evo-mermaid-${++renders}`;
        const { svg } = await mermaid.render(id, code);
        if (live) setState({ svg, id });
      } catch (error) {
        if (live) setState({ error: error instanceof Error ? error.message.split("\n")[0] : "It could not be parsed." });
      }
    })();
    return () => {
      live = false;
    };
  }, [code, theme]);

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
          title="Double-click to open full screen"
          className="flex justify-center overflow-x-auto [&_svg]:h-auto [&_svg]:max-w-full"
          onDoubleClick={() => setFull(true)}
          // Mermaid's strict mode sanitises its SVG (DOMPurify) before it returns it.
          dangerouslySetInnerHTML={{ __html: state.svg }}
        />
      ) : (
        <Skeleton className="h-40 w-full" />
      )}
      <div className="mt-2 flex gap-1">
        <Button variant="ghost" size="xs" className="text-muted-foreground" aria-pressed={source} onClick={() => setSource((value) => !value)}>
          {source ? "Show diagram" : "Show source"}
        </Button>
        {fullSvg && !source && (
          <Button variant="ghost" size="xs" className="text-muted-foreground" onClick={() => setFull(true)}>
            <Maximize2Icon data-icon="inline-start" />
            Full screen
          </Button>
        )}
      </div>
      {fullSvg && <DiagramViewer svg={fullSvg} open={full} onOpenChange={setFull} />}
    </BlockFrame>
  );
}
