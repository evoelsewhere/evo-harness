// The pages the sidebar can show as rows; the rest are in its More menu
// (docs/features/sidebar-items/spec.md, FR-1). A new page is one entry here:
// the sidebar row, the More menu and Edit sidebar all read this list.

import * as React from "react";
import { AppWindowIcon } from "lucide-react";
import { PluginsIcon, SchedulerIcon, UsageIcon } from "@/components/icons/tool-icons";
import { openConnectorsAt } from "@/features/connected-apps";
import { pluginIcon } from "@/features/plugin-ui";
import { usePluginUiStore, type PluginViewRef } from "@/stores/plugin-ui-store";
import { useUiStore } from "@/stores/ui-store";

type UiState = ReturnType<typeof useUiStore.getState>;

export interface SidebarItem {
  /** Stable: it keys the saved choice (`ui.sidebar.items`). */
  id: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  /** Shown as a row until the person chooses otherwise. */
  defaultShown: boolean;
  /** What the row and the More menu entry do. */
  open(): void;
  /** The item's page is open (marks its row, or More when it is hidden). */
  isActive(ui: UiState): boolean;
}

export const SIDEBAR_ITEMS: readonly SidebarItem[] = [
  {
    id: "scheduler",
    label: "Scheduler",
    icon: SchedulerIcon,
    defaultShown: false,
    open: () => useUiStore.getState().openScheduler(),
    isActive: (ui) => ui.schedulerOpen,
  },
  {
    id: "usage",
    label: "Usage",
    icon: UsageIcon,
    defaultShown: true,
    open: () => useUiStore.getState().openUsage(),
    isActive: (ui) => ui.usageOpen,
  },
  {
    id: "customize",
    label: "Customize",
    icon: PluginsIcon,
    defaultShown: false,
    open: () => useUiStore.getState().openCustomize(),
    isActive: (ui) => ui.customizeOpen,
  },
  {
    // docs/features/connected-apps/spec.md, FR-3.
    id: "connected-apps",
    label: "Connected apps",
    icon: AppWindowIcon,
    defaultShown: false,
    // Customize → Connectors, on the apps (connected-apps FR-3).
    open: () => openConnectorsAt("apps:browse"),
    isActive: () => false,
  },
];

/** Enabled plugins' sidebar pages as items (docs/features/plugin-ui/spec.md, FR-4). */
export function pluginSidebarItems(views: readonly PluginViewRef[]): SidebarItem[] {
  return views
    .filter((view) => view.slot === "sidebar")
    .map((view) => ({
      id: view.key,
      label: view.view.title,
      icon: pluginIcon(view.icon),
      defaultShown: view.view.defaultShown,
      open: () => useUiStore.getState().openPluginPage(view.key),
      isActive: (ui) => ui.pluginPage === view.key,
    }));
}

/** The fixed items, then the enabled plugins' pages. */
export function useSidebarItems(): readonly SidebarItem[] {
  const views = usePluginUiStore((s) => s.views);
  return React.useMemo(() => [...SIDEBAR_ITEMS, ...pluginSidebarItems(views)], [views]);
}

/** The person's choice for `item`, or its default when they made none. */
export const isShown = (item: SidebarItem, saved: Record<string, boolean>): boolean => saved[item.id] ?? item.defaultShown;
