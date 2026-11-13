// Every keyboard shortcut, in one list, for the shortcuts dialog
// (docs/features/ux-polish/spec.md, FR-7). The handlers live where they
// act (`app/use-app-bootstrap.ts`, the composer, the menus); a shortcut
// added there is added here too.

import { panels } from "@/features/panels/panel-registry";
import { isMac, modKey } from "@/lib/platform";

export interface Shortcut {
  label: string;
  /** Key caps, in order ("Ctrl", "N"). */
  keys: string[];
}

export interface ShortcutGroup {
  title: string;
  items: Shortcut[];
}

const mod = (...keys: string[]) => [modKey, ...keys];

export function shortcutGroups(): ShortcutGroup[] {
  return [
    {
      title: "General",
      items: [
        { label: "New chat", keys: mod("N") },
        { label: "Search chats and commands", keys: mod("P") },
        { label: "Open folder", keys: mod("O") },
        { label: "Settings", keys: mod(",") },
        { label: "Keyboard shortcuts", keys: mod("/") },
        { label: "Back / forward", keys: isMac ? ["⌘", "[ ]"] : ["Alt", "← →"] },
        { label: "Zoom in / out / reset", keys: mod("+ − 0") },
        { label: "Reload the window", keys: mod("R") },
        ...(isMac ? [] : [{ label: "Full screen", keys: ["F11"] }]),
      ],
    },
    {
      title: "Composer",
      items: [
        { label: "Send", keys: ["Enter"] },
        { label: "New line", keys: ["Shift", "Enter"] },
        { label: "Queue while the agent works", keys: ["Ctrl", "Enter"] },
        { label: "Commands and skills", keys: ["/"] },
        { label: "Mention a file", keys: ["@"] },
      ],
    },
    {
      title: "Side panel",
      items: [
        { label: "Show or hide the side panel", keys: mod("J") },
        { label: "Next / previous tab", keys: ["Ctrl", "Tab"] },
        { label: "Terminal", keys: ["Ctrl", "`"] },
        { label: "New terminal", keys: ["Ctrl", "Shift", "`"] },
        ...panels.filter((panel) => panel.shortcut !== "`").map((panel) => ({ label: panel.title, keys: mod(panel.shortcut.toUpperCase()) })),
      ],
    },
    {
      title: "Split view",
      items: [
        { label: "Focus the left / right pane", keys: mod("1 2") },
        { label: "Close the focused pane", keys: mod("\\") },
      ],
    },
    {
      title: "Chat menu (right-click a chat)",
      items: [
        { label: "Pin · Mark as unread · Rename", keys: ["P", "U", "R"] },
        { label: "Fork · Archive · Delete", keys: ["F", "A", "D"] },
        { label: "Open in a new window · Split view", keys: ["1", "S"] },
      ],
    },
  ];
}
