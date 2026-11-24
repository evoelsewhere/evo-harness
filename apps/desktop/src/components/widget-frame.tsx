// A `show_widget` widget in the chat (docs/features/office-v3-parity/spec.md,
// FR-13; port of v3 web/src/components/WidgetRenderer.tsx).
//
// The model's HTML runs in an iframe with `sandbox="allow-scripts"` only: an
// opaque origin, so it cannot reach the app, its storage or the engine. (v3
// also granted `allow-same-origin`, which lets a frame's script reach its
// parent; evo leaves it out and talks to the frame by postMessage.)
//
// In the app the document comes from the shell's `evo-widget` scheme, which
// sends the v3 widget policy as its header (ADR 0027). A `data:` frame would
// inherit the app's CSP, whose `script-src` lets none of the widget's
// scripts run. Outside Tauri (the Vite preview, tests) there is no app
// policy, and the frame loads a `data:` URL.
//
// Messages are accepted only from this frame's own window: `sendPrompt(text)`
// sends `text` as a new message in the chat, and the height report sizes the
// frame (120–2400 px). The app's theme is seeded into the document and kept
// in sync.

import * as React from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { Spinner } from "@/components/ui/spinner";
import { inTauri } from "@/lib/platform";
import {
  WIDGET_RESIZE,
  WIDGET_SEND_PROMPT,
  WIDGET_THEME,
  clampWidgetHeight,
  readWidgetTheme,
  wrapWidgetHtml,
} from "@/lib/widget-document";
import { cn } from "@/lib/utils";
import { sendOrSteer } from "@/stores/engine-store";

/** The scheme the shell serves widget documents from (`src-tauri/src/widgets.rs`). */
const WIDGET_SCHEME = "evo-widget";

/**
 * The frame's URL for `html`, wrapped with the app's theme: the shell's
 * `evo-widget` document in the app (closed again when the frame goes or the
 * HTML changes), a `data:` URL elsewhere. Empty while there is none yet.
 */
function useWidgetSrc(html: string): string {
  const [src, setSrc] = React.useState("");
  React.useEffect(() => {
    if (!html) {
      setSrc("");
      return;
    }
    const { theme, colorScheme } = appTheme();
    const document = wrapWidgetHtml(html, theme, colorScheme);
    if (!inTauri) {
      setSrc(`data:text/html;charset=utf-8,${encodeURIComponent(document)}`);
      return;
    }
    let id: string | undefined;
    let gone = false;
    setSrc("");
    invoke<string>("widget_open", { html: document })
      .then((opened) => {
        id = opened;
        if (gone) void invoke("widget_close", { id: opened }).catch(() => undefined);
        else setSrc(convertFileSrc(opened, WIDGET_SCHEME));
      })
      .catch(() => {
        // The shell refused it (too large): the frame shows it without its scripts.
        if (!gone) setSrc(`data:text/html;charset=utf-8,${encodeURIComponent(document)}`);
      });
    return () => {
      gone = true;
      if (id) void invoke("widget_close", { id }).catch(() => undefined);
    };
  }, [html]);
  return src;
}

function appTheme() {
  const root = document.documentElement;
  return {
    theme: readWidgetTheme(window.getComputedStyle(root)),
    colorScheme: root.classList.contains("dark") ? ("dark" as const) : ("light" as const),
  };
}


export interface WidgetFrameProps {
  /** The widget's HTML (`widget_code`); empty while it is still being written. */
  html: string;
  title?: string;
  /** Height before the widget reports its own. */
  height?: number;
  /** Shown while `html` is empty. */
  loadingMessages?: string[];
  className?: string;
  /** Where `sendPrompt(text)` goes; the active chat by default. */
  onSendPrompt?(text: string): void;
}

export function WidgetFrame({
  html,
  title = "Widget",
  height = 600,
  loadingMessages,
  className,
  onSendPrompt,
}: WidgetFrameProps) {
  const frameRef = React.useRef<HTMLIFrameElement>(null);
  const [contentHeight, setContentHeight] = React.useState<number | null>(null);
  const sendRef = React.useRef(onSendPrompt);
  sendRef.current = onSendPrompt;

  // The theme at load time is baked into the document; later changes are posted.
  const src = useWidgetSrc(html);

  React.useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      // Widgets speak only through their own frame: any other window could
      // otherwise send a prompt into the chat.
      const frame = frameRef.current?.contentWindow;
      if (!frame || event.source !== frame) return;
      const data = event.data as { type?: unknown; prompt?: unknown; height?: unknown } | null;
      if (data?.type === WIDGET_SEND_PROMPT && typeof data.prompt === "string" && data.prompt.trim()) {
        (sendRef.current ?? sendOrSteer)(data.prompt.trim());
      } else if (data?.type === WIDGET_RESIZE && typeof data.height === "number" && Number.isFinite(data.height)) {
        setContentHeight(clampWidgetHeight(data.height));
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  // The frame's document does not inherit the app's variables: post them
  // whenever the appearance changes — theme (`class`), accent and fonts
  // (`style`), background colour and material (`data-bg-*`, `data-material`),
  // see stores/appearance-store.ts.
  React.useEffect(() => {
    const sync = () => {
      const { theme, colorScheme } = appTheme();
      frameRef.current?.contentWindow?.postMessage({ type: WIDGET_THEME, theme, colorScheme }, "*");
    };
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style", "data-bg-light", "data-bg-dark", "data-material"],
    });
    if (document.body) observer.observe(document.body, { attributes: true, attributeFilter: ["class", "style"] });
    return () => observer.disconnect();
  }, []);

  if (!html || !src) {
    return (
      <div
        className={cn("flex w-full items-center justify-center rounded-lg bg-card shadow-border", className)}
        style={{ minHeight: Math.min(height, 240) }}
        role="status"
      >
        <div className="flex flex-col items-center gap-2 text-muted-foreground">
          <Spinner />
          <span className="shimmer text-sm">{loadingMessages?.[0] || "Loading visualization..."}</span>
        </div>
      </div>
    );
  }

  return (
    <div className={cn("relative overflow-hidden", className)}>
      <iframe
        ref={frameRef}
        title={title}
        src={src}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        scrolling="no"
        style={{ height: contentHeight ?? clampWidgetHeight(height) }}
        className="block w-full border-0 transition-[height] duration-150"
      />
    </div>
  );
}
