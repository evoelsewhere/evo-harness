// The enabled plugins' views, by slot (docs/features/plugin-ui/spec.md,
// FR-4 – FR-7, FR-12, FR-15). Read from `plugins_state` and again on every
// `plugins://changed`, so a plugin turned off loses its slots at once.
//
// `startPluginUi` also connects the window to its views: the app's theme
// goes to every view (FR-12), and what a view asks of the window — open a
// page or panel, a notification, text for the composer — arrives here as
// `plugin-ui://…` events (src-tauri/src/plugin_ui/bridge.rs).

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { notify } from "@/features/notifications/notify";
import { log } from "@/lib/db";
import { inTauri } from "@/lib/platform";
import {
  PLUGIN_VIEW_EVENTS,
  PLUGINS_CHANGED,
  plugins,
  pluginViews,
  type InstalledPlugin,
  type PluginSlotKind,
  type PluginView,
} from "@/lib/plugins";
import type { NotificationLevel } from "./notification-store";
import { useUiStore } from "./ui-store";

/** One view of an enabled plugin, in its slot. */
export interface PluginViewRef {
  /** `plugin:<plugin id>/<view id>`: the panel id and sidebar item id. */
  key: string;
  plugin: string;
  pluginName: string;
  slot: PluginSlotKind;
  view: PluginView;
  /** The icon as a data URL, when it has one. */
  icon?: string;
  /** Its plugin has a backend that is to run: the view waits for it (FR-17). */
  backend: boolean;
}

/** Whether `plugin` has a backend that is to run: not set to never start, nothing waiting on the person. */
export function runsBackend(plugin: InstalledPlugin): boolean {
  if (!plugin.runtime) return false;
  if ((plugin.startMode ?? plugin.runtime.startup) === "off") return false;
  return !(plugin.waiting ?? []).some((w) => w.part === "setup" || w.part === "runtime");
}

/** `plugin:<plugin id>/<view id>`. */
export const pluginViewKey = (plugin: string, view: string) => `plugin:${plugin}/${view}`;

/** The plugin and view of a `plugin:` key; `null` for any other id. */
export function parsePluginViewKey(key: string): { plugin: string; view: string } | null {
  if (!key.startsWith("plugin:")) return null;
  const rest = key.slice("plugin:".length);
  const slash = rest.lastIndexOf("/");
  if (slash <= 0 || slash === rest.length - 1) return null;
  return { plugin: rest.slice(0, slash), view: rest.slice(slash + 1) };
}

const SLOTS: readonly PluginSlotKind[] = ["sidebar", "panels", "cards", "composer"];

/** The views of the enabled plugins, slot by slot, in install order. */
export function viewsFrom(installed: InstalledPlugin[]): PluginViewRef[] {
  const views: PluginViewRef[] = [];
  for (const plugin of installed) {
    if (!plugin.enabled || !plugin.ui) continue;
    const backend = runsBackend(plugin);
    for (const slot of SLOTS) {
      for (const view of plugin.ui[slot] ?? []) {
        views.push({
          key: pluginViewKey(plugin.id, view.id),
          plugin: plugin.id,
          pluginName: plugin.name,
          slot,
          view,
          icon: view.icon ? plugin.icons?.[view.icon] : undefined,
          backend,
        });
      }
    }
  }
  return views;
}

interface PluginUiState {
  views: PluginViewRef[];
  /** `plugins_state` was read at least once. */
  loaded: boolean;
  load(): Promise<void>;
}

export const usePluginUiStore = create<PluginUiState>()((set) => ({
  views: [],
  loaded: false,
  load: async () => {
    if (!inTauri) return;
    try {
      const state = await plugins.state();
      set({ views: viewsFrom(state.installed), loaded: true });
    } catch (error) {
      log("warn", "plugins", `could not read the plugins' views: ${String(error)}`);
      set({ loaded: true });
    }
  },
}));

/** The view with `key` (`plugin:<id>/<view>`) in `slot`, if its plugin is on. */
export const findView = (views: PluginViewRef[], key: string, slot?: PluginSlotKind) =>
  views.find((view) => view.key === key && (!slot || view.slot === slot));

// ── Theme (FR-12) ───────────────────────────────────────────────────────────

/** The app's tokens a view gets, as `--evo-<name>`. */
const THEME_TOKENS = [
  "background",
  "foreground",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "destructive",
  "border",
  "input",
  "ring",
  "brand",
  "brand-foreground",
  "link",
  "radius",
  "font-sans",
  "font-mono",
] as const;

/** The theme as views get it: `{tokens, dark}` read from the root element. */
export function readTheme(): { tokens: Record<string, string>; dark: boolean } {
  const style = getComputedStyle(document.documentElement);
  const tokens: Record<string, string> = {};
  for (const name of THEME_TOKENS) {
    const value = style.getPropertyValue(`--${name}`).trim();
    if (value) tokens[name] = value;
  }
  return { tokens, dark: document.documentElement.classList.contains("dark") };
}

let started = false;

/**
 * Read the views and follow plugin changes; connect this window to its
 * views' events. The main window also sends the theme, now and on change.
 */
export function startPluginUi() {
  if (started || !inTauri) return;
  started = true;
  // A reloaded window's earlier views would sit over the new page: they
  // close before this page's slots (which need the views listed) open any.
  const { load } = usePluginUiStore.getState();
  void pluginViews
    .reset()
    .catch(() => undefined)
    .then(() => load());
  void listen(PLUGINS_CHANGED, () => void load());

  const windowLabel = getCurrentWindow().label;
  if (windowLabel === "main") {
    let last = "";
    const send = () => {
      const theme = readTheme();
      const key = JSON.stringify(theme);
      if (key === last) return;
      last = key;
      void pluginViews.theme(theme.tokens, theme.dark).catch(() => undefined);
    };
    send();
    new MutationObserver(send).observe(document.documentElement, { attributes: true, attributeFilter: ["class", "style", "data-theme"] });
  }

  // A view opens one of its plugin's pages or panels (`ui.open`, `plugin_open`).
  void listen<{ plugin: string; view: string; slot: PluginSlotKind }>(PLUGIN_VIEW_EVENTS.open, ({ payload }) => {
    const key = pluginViewKey(payload.plugin, payload.view);
    if (payload.slot === "sidebar") useUiStore.getState().openPluginPage(key);
    else if (payload.slot === "panels") useUiStore.getState().openPanel(key as `plugin:${string}`);
  });
  void listen<{ plugin: string; name: string; title?: string | null; body: string; level: NotificationLevel }>(
    PLUGIN_VIEW_EVENTS.notify,
    ({ payload }) => {
      notify({ title: payload.title || payload.name, body: payload.body, level: payload.level, source: "plugins", key: undefined });
    },
  );
  void listen<{ plugin: string; text: string }>(PLUGIN_VIEW_EVENTS.composerInsert, ({ payload }) => {
    void import("@/features/chat/chat-composer").then(({ composerDraft }) => composerDraft.insert(payload.text));
  });
}
