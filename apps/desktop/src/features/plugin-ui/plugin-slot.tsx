// The place a plugin view fills (docs/features/plugin-ui/spec.md, FR-4 – FR-7,
// FR-9, FR-15; plan 3.5). The view is a native webview the shell lays over
// this placeholder (src-tauri/src/plugin_ui/views.rs): the slot opens it,
// keeps it on its box as things move (scrolling, resizing, split view, the
// side panel sliding), hides it while a menu or dialog covers it or while it
// is not wholly inside its scroll area, and closes it when the slot goes.
// Behind the view the slot shows the plugin's icon and title: that is what
// shows while it loads, while it is hidden, and after it closed.

import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { PuzzleIcon, RotateCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { PLUGIN_VIEW_EVENTS, pluginViews, type PluginSlotKind, type ViewBounds } from "@/lib/plugins";
import { inTauri } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

/** Popups (menus, dialogs, popovers) that would sit under a native view. */
const OVERLAYS =
  '[data-slot="dropdown-menu-content"],[data-slot="context-menu-content"],[data-slot="popover-content"],[data-slot="select-content"],[data-slot="hover-card-content"],[data-slot="tooltip-content"],[role="dialog"],[role="alertdialog"]';

// ── What the shell says about views, by label ───────────────────────────────

type ViewNews = { loaded?: boolean; closed?: "off" | "cap"; height?: number };
const news = new Map<string, ViewNews>();
const watchers = new Set<() => void>();
let listening = false;

function follow() {
  if (listening || !inTauri) return;
  listening = true;
  const update = (label: string, change: ViewNews) => {
    news.set(label, { ...news.get(label), ...change });
    for (const watcher of watchers) watcher();
  };
  void listen<{ label: string }>(PLUGIN_VIEW_EVENTS.loaded, ({ payload }) => update(payload.label, { loaded: true }));
  void listen<{ label: string; reason: "off" | "cap" }>(PLUGIN_VIEW_EVENTS.closed, ({ payload }) => update(payload.label, { closed: payload.reason }));
  void listen<{ label: string; height: number }>(PLUGIN_VIEW_EVENTS.resize, ({ payload }) => update(payload.label, { height: payload.height }));
}

function useViewNews(label: string | null): ViewNews {
  const [, redraw] = React.useReducer((n: number) => n + 1, 0);
  React.useEffect(() => {
    follow();
    watchers.add(redraw);
    return () => void watchers.delete(redraw);
  }, []);
  return (label && news.get(label)) || {};
}

// ── Views kept while out of sight ───────────────────────────────────────────
//
// Creating a webview takes most of a second; a page or panel the person
// leaves stays open, hidden, for a while, so coming back is instant and
// finds the page as it was (plan 3.5; NFR-2).

const KEEP_MS = 5 * 60_000;
const kept = new Map<string, { label: string; generation: number; timer: ReturnType<typeof setTimeout> }>();

function keep(key: string, label: string, generation: number) {
  const earlier = kept.get(key);
  if (earlier && earlier.label !== label) closeLabel(earlier.label);
  if (earlier) clearTimeout(earlier.timer);
  void pluginViews.visible(label, false).catch(() => undefined);
  const timer = setTimeout(() => {
    if (kept.get(key)?.label === label) {
      kept.delete(key);
      closeLabel(label);
    }
  }, KEEP_MS);
  kept.set(key, { label, generation, timer });
}

/** A kept view of `key` opened at `generation`, taken back; one of another generation is closed. */
function takeKept(key: string, generation: number): string | null {
  const entry = kept.get(key);
  if (!entry) return null;
  kept.delete(key);
  clearTimeout(entry.timer);
  if (entry.generation !== generation || news.get(entry.label)?.closed) {
    closeLabel(entry.label);
    return null;
  }
  return entry.label;
}

function closeLabel(label: string) {
  news.delete(label);
  void pluginViews.close(label).catch(() => undefined);
}

// ── Measuring ───────────────────────────────────────────────────────────────

/** The box the slot is seen through: its scroll areas' boxes and the window's, intersected. */
function clipOf(element: HTMLElement): DOMRect {
  let top = 0;
  let left = 0;
  let right = window.innerWidth;
  let bottom = window.innerHeight;
  for (let parent = element.parentElement; parent && parent !== document.body; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (/(auto|scroll|hidden|clip)/.test(style.overflowY + style.overflowX)) {
      const box = parent.getBoundingClientRect();
      top = Math.max(top, box.top);
      left = Math.max(left, box.left);
      right = Math.min(right, box.right);
      bottom = Math.min(bottom, box.bottom);
    }
  }
  return new DOMRect(left, top, Math.max(0, right - left), Math.max(0, bottom - top));
}

export interface Placement {
  bounds: ViewBounds;
  /** Wholly in sight and not covered. */
  visible: boolean;
  /** Within a screen of its scroll area (cards close beyond that). */
  near: boolean;
}

/** Where the slot is, and whether its view may show (pure but for the DOM reads). */
export function placementOf(element: HTMLElement, scale: number): Placement {
  const rect = element.getBoundingClientRect();
  const clip = clipOf(element);
  const bounds = { x: rect.left * scale, y: rect.top * scale, width: rect.width * scale, height: rect.height * scale };
  const sized = rect.width >= 2 && rect.height >= 2 && element.isConnected && element.offsetParent !== null;
  const inside = rect.top >= clip.top - 1 && rect.bottom <= clip.bottom + 1 && rect.left >= clip.left - 1 && rect.right <= clip.right + 1;
  const covered = [...document.querySelectorAll<HTMLElement>(OVERLAYS)].some((popup) => {
    // A popover or sheet that holds the slot does not cover it.
    if (popup.contains(element)) return false;
    const box = popup.getBoundingClientRect();
    return box.width > 0 && box.right > rect.left && box.left < rect.right && box.bottom > rect.top && box.top < rect.bottom;
  });
  const reach = Math.max(clip.height, 200);
  const near = sized && rect.bottom > clip.top - reach && rect.top < clip.bottom + reach;
  return { bounds, visible: sized && inside && !covered && document.visibilityState === "visible", near };
}

// ── The slot ────────────────────────────────────────────────────────────────

export interface PluginSlotProps {
  plugin: string;
  pluginName: string;
  slot: PluginSlotKind;
  view: string;
  title: string;
  /** The icon as a data URL. */
  icon?: string;
  /** A card's data (`evo.view.data()`). */
  data?: unknown;
  /** Close the view while the slot is more than a screen out of sight (cards). */
  unloadOffscreen?: boolean;
  /** The view asked for a height (`evo.view.resize`). */
  onHeight?(height: number): void;
  /** Bump to open the view afresh (a Reload button). */
  generation?: number;
  /** Keep the view, hidden, for a while after the slot goes (pages and panels). */
  keepAlive?: boolean;
  className?: string;
}

export function PluginSlot({ plugin, pluginName, slot, view, title, icon, data, unloadOffscreen, onHeight, generation = 0, keepAlive, className }: PluginSlotProps) {
  const area = React.useRef<HTMLDivElement>(null);
  const zoom = useUiStore((s) => s.zoom);
  const [label, setLabel] = React.useState<string | null>(null);
  const [failure, setFailure] = React.useState<string | null>(null);
  const [near, setNear] = React.useState(true);
  const [attempt, setAttempt] = React.useState(0);
  const labelRef = React.useRef<string | null>(null);
  const sent = React.useRef<{ bounds: string; visible: boolean | null }>({ bounds: "", visible: null });
  const viewNews = useViewNews(label);
  const dataRef = React.useRef(data);
  dataRef.current = data;
  const live = !unloadOffscreen || near;

  // Open the view (again when the slot comes back into reach, or on Reopen).
  React.useEffect(() => {
    const element = area.current;
    if (!element || !live || !inTauri) return;
    let cancelled = false;
    const place = placementOf(element, useUiStore.getState().zoom || 1);
    setFailure(null);
    const keyOfView = keepAlive ? `${slot}:${plugin}/${view}` : null;
    const reused = keyOfView ? takeKept(keyOfView, generation) : null;
    const done = () => {
      cancelled = true;
      const open = labelRef.current;
      labelRef.current = null;
      setLabel(null);
      if (!open) return;
      if (keyOfView && !news.get(open)?.closed) keep(keyOfView, open, generation);
      else closeLabel(open);
    };
    if (reused) {
      labelRef.current = reused;
      // The measuring below places it and shows it.
      sent.current = { bounds: "", visible: null };
      setLabel(reused);
      return done;
    }
    pluginViews
      .open(plugin, slot, view, place.bounds, place.visible, dataRef.current)
      .then((opened) => {
        if (cancelled) {
          void pluginViews.close(opened).catch(() => undefined);
          return;
        }
        labelRef.current = opened;
        sent.current = { bounds: JSON.stringify(place.bounds), visible: place.visible };
        setLabel(opened);
      })
      .catch((error) => !cancelled && setFailure(String(error)));
    return done;
  }, [plugin, slot, view, live, attempt, generation, keepAlive]);

  // Keep the view on the slot's box, shown only when it can be seen whole.
  React.useLayoutEffect(() => {
    const element = area.current;
    if (!element) return;
    let frame = 0;
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const place = placementOf(element, useUiStore.getState().zoom || 1);
        if (unloadOffscreen) setNear(place.near);
        const open = labelRef.current;
        if (!open) return;
        const bounds = JSON.stringify(place.bounds);
        if (bounds !== sent.current.bounds) {
          sent.current.bounds = bounds;
          void pluginViews.bounds(open, place.bounds).catch(() => undefined);
        }
        if (place.visible !== sent.current.visible) {
          sent.current.visible = place.visible;
          void pluginViews.visible(open, place.visible).catch(() => undefined);
        }
      });
    };
    measure();
    const resize = new ResizeObserver(measure);
    resize.observe(element);
    resize.observe(document.documentElement);
    const mutations = new MutationObserver(measure);
    mutations.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-open", "data-closed", "style", "class"] });
    window.addEventListener("resize", measure);
    // Any scroll area may move the slot.
    window.addEventListener("scroll", measure, true);
    document.addEventListener("visibilitychange", measure);
    // The side panel slides, a popover grows in: follow for a moment.
    const settle = setInterval(measure, 100);
    const stop = setTimeout(() => clearInterval(settle), 600);
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      mutations.disconnect();
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
      document.removeEventListener("visibilitychange", measure);
      clearInterval(settle);
      clearTimeout(stop);
    };
  }, [zoom, label, unloadOffscreen]);

  React.useEffect(() => {
    if (viewNews.height && onHeight) onHeight(viewNews.height);
  }, [viewNews.height, onHeight]);

  const closed = viewNews.closed;
  const loading = live && !failure && !closed && !viewNews.loaded;
  return (
    <div ref={area} className={cn("relative h-full w-full overflow-hidden", className)} data-plugin-slot={`${plugin}/${view}`}>
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-4 text-center">
        <span className="flex size-9 items-center justify-center rounded-lg bg-muted text-muted-foreground">
          {icon ? <img src={icon} alt="" className="size-5" /> : <PuzzleIcon className="size-4" />}
        </span>
        <p className="text-[13px] font-medium">{title}</p>
        {failure ? (
          <>
            <p className="max-w-72 text-xs text-muted-foreground">
              {pluginName} could not open this view: {failure}
            </p>
            <Button size="sm" variant="outline" onClick={() => setAttempt((n) => n + 1)}>
              <RotateCwIcon /> Try again
            </Button>
          </>
        ) : closed === "off" ? (
          <p className="max-w-72 text-xs text-muted-foreground">{pluginName} is off.</p>
        ) : closed === "cap" ? (
          <Button size="sm" variant="outline" onClick={() => setAttempt((n) => n + 1)}>
            Show again
          </Button>
        ) : loading ? (
          <Spinner className="size-4 text-muted-foreground" />
        ) : (
          <p className="text-xs text-muted-foreground">{pluginName}</p>
        )}
      </div>
    </div>
  );
}
