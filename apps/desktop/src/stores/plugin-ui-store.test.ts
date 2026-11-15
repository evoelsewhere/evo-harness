// docs/features/plugin-ui/spec.md (FR-4, FR-5, FR-15).
import { describe, expect, test } from "bun:test";
import type { InstalledPlugin, PluginView } from "@/lib/plugins";
import { pluginPanels } from "@/features/panels/panel-registry";
import { pluginSidebarItems } from "@/features/shell/sidebar-items";
import { parsePluginViewKey, pluginViewKey, viewsFrom } from "./plugin-ui-store";

const view = (id: string, extra: Partial<PluginView> = {}): PluginView => ({ id, title: id.toUpperCase(), entry: `ui/${id}.html`, defaultShown: false, ...extra });

const plugin = (id: string, enabled: boolean, ui: Partial<InstalledPlugin["ui"]>, icons: Record<string, string> = {}) =>
  ({ id, name: id.split("@")[0], enabled, ui: { sidebar: [], panels: [], cards: [], composer: [], ...ui }, icons }) as unknown as InstalledPlugin;

describe("plugin views", () => {
  test("keys name the plugin and the view, and read back", () => {
    const key = pluginViewKey("sticky-wall@local", "wall");
    expect(key).toBe("plugin:sticky-wall@local/wall");
    expect(parsePluginViewKey(key)).toEqual({ plugin: "sticky-wall@local", view: "wall" });
    expect(parsePluginViewKey("terminal")).toBeNull();
    expect(parsePluginViewKey("plugin:/wall")).toBeNull();
    expect(parsePluginViewKey("plugin:x@y/")).toBeNull();
  });

  test("only enabled plugins' views, slot by slot, with their icons", () => {
    const views = viewsFrom([
      plugin("wall@local", true, { sidebar: [view("wall", { icon: "ui/icon.svg" })], panels: [view("notes")], cards: [view("chart")] }, { "ui/icon.svg": "data:image/svg+xml;base64,AA==" }),
      plugin("off@local", false, { sidebar: [view("hidden")] }),
    ]);
    expect(views.map((v) => `${v.slot}:${v.key}`)).toEqual([
      "sidebar:plugin:wall@local/wall",
      "panels:plugin:wall@local/notes",
      "cards:plugin:wall@local/chart",
    ]);
    expect(views[0].icon).toStartWith("data:image/svg+xml");
    expect(views[1].icon).toBeUndefined();
  });

  test("sidebar pages become sidebar items that open the page", () => {
    const items = pluginSidebarItems(viewsFrom([plugin("wall@local", true, { sidebar: [view("wall", { defaultShown: true })], panels: [view("notes")] })]));
    expect(items.map((item) => [item.id, item.label, item.defaultShown])).toEqual([["plugin:wall@local/wall", "WALL", true]]);
    expect(items[0].isActive({ pluginPage: "plugin:wall@local/wall" } as never)).toBe(true);
  });

  test("plugin panels keep a free shortcut and lose one a built-in tool has", () => {
    const panels = pluginPanels(
      viewsFrom([plugin("wall@local", true, { panels: [view("notes", { shortcut: "K" }), view("files", { shortcut: "F" })] })]),
    );
    expect(panels.map((p) => [p.id, p.shortcut, p.plugin])).toEqual([
      ["plugin:wall@local/notes", "K", "wall@local"],
      ["plugin:wall@local/files", "", "wall@local"],
    ]);
  });
});
