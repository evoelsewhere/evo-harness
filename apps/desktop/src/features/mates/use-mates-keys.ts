// Evo Mates' keyboard (docs/features/evo-mates-ui-v2/spec.md, FR-8; evo-mates
// FR-38): one listener for the window. Ctrl+K switcher, Ctrl+N new mate,
// Ctrl+1 – 9 pinned mates, Ctrl+B fold the sidebar, Ctrl+I profile or
// members, Ctrl+F find, Ctrl+/ shortcuts, Alt+↑/↓ previous / next place,
// Alt+Shift+↑/↓ previous / next unread or waiting one, Esc closes. ⌘ on macOS.

import * as React from "react";
import { modKey } from "@/lib/platform";
import { useMatesStore, type MatesView } from "@/stores/mates-store";
import { useMatesUiStore } from "@/stores/mates-ui-store";
import { useSessionStore } from "@/stores/session-store";
import { placeKey, sidebarOrder, stepPlace, type Place } from "./sidebar-layout";

/** The place the window shows, as the sidebar's order names it. */
export function currentPlace(view: MatesView): string | null {
  switch (view.kind) {
    case "mate":
      return `mate:${view.id}`;
    case "channel":
      return `channel:${view.id}`;
    case "files":
      return "files";
    case "today":
    case "empty":
      return "today";
    default:
      return null;
  }
}

export function openPlace(place: Place) {
  const store = useMatesStore.getState();
  if (place.kind === "today") store.showToday();
  else if (place.kind === "files") store.showFiles();
  else if (place.kind === "mate") void store.open(place.id);
  else void store.openChannel(place.id);
}

/** A dialog or a menu holds the keyboard. */
function overlayOpen(): boolean {
  return Boolean(document.querySelector("[role=dialog],[role=menu],[role=alertdialog]"));
}

export function useMatesKeys() {
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const ui = useMatesUiStore.getState();
      const store = useMatesStore.getState();
      if (event.key === "Escape") {
        if (event.defaultPrevented || overlayOpen()) return;
        // Find closes first, then the profile (FR-15, evo-mates FR-38).
        if (ui.findOpen) ui.setFindOpen(false);
        else if (ui.membersOpen) ui.setMembersOpen(false);
        else store.setPanelOpen(false);
        return;
      }
      const mod = modKey === "⌘" ? event.metaKey : event.ctrlKey;
      // Alt+arrows walk the sidebar (FR-8).
      if (event.altKey && !mod && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
        if (overlayOpen()) return;
        event.preventDefault();
        const order = sidebarOrder(store.mates, store.channels);
        const sessions = useSessionStore.getState();
        const attention = event.shiftKey
          ? (chat: string) => sessions.work[chat] === "waiting" || Boolean(sessions.sessions.find((c) => c.id === chat)?.unread)
          : undefined;
        const next = stepPlace(order, currentPlace(store.view), event.key === "ArrowDown" ? 1 : -1, attention);
        if (next) openPlace(next);
        return;
      }
      if (!mod || event.altKey) return;
      const key = event.key.toLowerCase();
      // Ctrl+K toggles the switcher even over itself.
      if (key === "k") {
        event.preventDefault();
        ui.setSwitcherOpen(!ui.switcherOpen);
        return;
      }
      if (overlayOpen()) return;
      if (key === "n") {
        event.preventDefault();
        store.showGallery();
      } else if (key === "b") {
        event.preventDefault();
        ui.toggleSidebar();
      } else if (key === "i") {
        event.preventDefault();
        if (store.view.kind === "mate") store.setPanelOpen(!store.panelOpen);
        else if (store.view.kind === "channel") ui.setMembersOpen(!ui.membersOpen);
      } else if (key === "f") {
        if (store.view.kind !== "mate" && store.view.kind !== "channel") return;
        event.preventDefault();
        ui.setFindOpen(true);
        // The bar takes the focus when it is already open.
        window.dispatchEvent(new Event("mates:focus-find"));
      } else if (key === "/" || key === "?") {
        event.preventDefault();
        ui.setShortcutsOpen(!ui.shortcutsOpen);
      } else if (/^[1-9]$/.test(key)) {
        // Ctrl+1 – 9 open the pinned mates, in the roster's order (evo-mates FR-38).
        const pinned = store.mates.filter((m) => m.pinned && !m.hidden);
        const mate = pinned[Number(key) - 1];
        if (mate) {
          event.preventDefault();
          void store.open(mate.id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

/** The shortcuts the sheet lists (FR-8). */
export const SHORTCUTS: { keys: string[]; label: string; group: "Move" | "Conversation" | "Window" }[] = [
  { keys: [modKey, "K"], label: "Go to a mate, channel or action", group: "Move" },
  { keys: ["Alt", "↑ / ↓"], label: "Previous / next in the sidebar", group: "Move" },
  { keys: ["Alt", "Shift", "↑ / ↓"], label: "Previous / next unread or waiting", group: "Move" },
  { keys: [modKey, "1 – 9"], label: "Open a pinned mate", group: "Move" },
  { keys: [modKey, "N"], label: "New mate", group: "Move" },
  { keys: [modKey, "F"], label: "Find in the conversation", group: "Conversation" },
  { keys: ["Enter"], label: "Send", group: "Conversation" },
  { keys: ["Shift", "Enter"], label: "New line", group: "Conversation" },
  { keys: ["@"], label: "Mention a mate in a channel", group: "Conversation" },
  { keys: [modKey, "I"], label: "Profile or channel members", group: "Window" },
  { keys: [modKey, "B"], label: "Fold or unfold the sidebar", group: "Window" },
  { keys: [modKey, "/"], label: "These shortcuts", group: "Window" },
  { keys: ["Esc"], label: "Close find, the members or the profile", group: "Window" },
];

export { placeKey };
