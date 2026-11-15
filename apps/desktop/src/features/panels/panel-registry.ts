import * as React from "react";
import {
  BrowserIcon,
  FilesIcon,
  MemoryIcon,
  SideChatIcon,
  TerminalIcon,
} from "@/components/icons/tool-icons";
import { pluginIcon } from "@/features/plugin-ui";
import { usePluginUiStore, type PluginViewRef } from "@/stores/plugin-ui-store";
import type { PanelId } from "@/stores/ui-store";

export interface PanelDefinition {
  id: PanelId;
  title: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  /** Key pressed with the primary modifier; none for a plugin panel without one. */
  shortcut: string;
  /** A plugin's panel (docs/features/plugin-ui/spec.md, FR-5). */
  plugin?: string;
}

/** Tools that open in the side panel. Add a panel by adding an entry here. */
export const panels: PanelDefinition[] = [
  { id: "terminal", title: "Terminal", description: "Run commands in the active workspace", icon: TerminalIcon, shortcut: "`" },
  { id: "browser", title: "Browser", description: "View and interact with the agent browser", icon: BrowserIcon, shortcut: "T" },
  { id: "files", title: "Files", description: "Browse workspace files and generated artifacts", icon: FilesIcon, shortcut: "F" },
  { id: "side-chat", title: "Side chat", description: "Ask a focused question without interrupting the run", icon: SideChatIcon, shortcut: ";" },
  { id: "memory", title: "Memory", description: "What the agent remembers across chats", icon: MemoryIcon, shortcut: "M" },
];

const builtinKeys = new Set(panels.map((panel) => panel.shortcut.toLowerCase()));

/** Enabled plugins' panels: a shortcut that a built-in tool has is dropped. */
export function pluginPanels(views: readonly PluginViewRef[]): PanelDefinition[] {
  return views
    .filter((view) => view.slot === "panels")
    .map((view) => ({
      id: view.key as PanelId,
      title: view.view.title,
      description: view.pluginName,
      icon: pluginIcon(view.icon),
      shortcut: view.view.shortcut && !builtinKeys.has(view.view.shortcut.toLowerCase()) ? view.view.shortcut : "",
      plugin: view.plugin,
    }));
}

/** The built-in tools, then the enabled plugins' panels (read now). */
export const allPanels = (): PanelDefinition[] => [...panels, ...pluginPanels(usePluginUiStore.getState().views)];

/** The built-in tools, then the enabled plugins' panels. */
export function usePanels(): PanelDefinition[] {
  const views = usePluginUiStore((s) => s.views);
  return React.useMemo(() => [...panels, ...pluginPanels(views)], [views]);
}

export const panelById = (id: PanelId | null, list: readonly PanelDefinition[] = panels) => list.find((panel) => panel.id === id);
