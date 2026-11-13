import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { panels } from "@/features/panels/panel-registry";
import { applyZoom, reloadWindow, resetZoom, toggleDevtools, toggleFullscreen, zoomBy } from "@/features/shell/app-commands";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { startShellNotices } from "@/features/notifications/notify";
import { installErrorLogging } from "@/lib/db";
import { hasMod, inTauri, isMac } from "@/lib/platform";
import { useHistoryStore } from "@/stores/history-store";
import { startChatScope } from "@/stores/chat-scope";
import { startSplit, useSplitStore } from "@/stores/split-store";
import { useEngineStore } from "@/stores/engine-store";
import { useTerminalStore } from "@/stores/terminal-store";
import { useUiStore } from "@/stores/ui-store";
import { startUpdates } from "@/stores/updates-store";
import { chatWindowId } from "@/lib/windows";

/** Connect the engine and read the window material once at startup. */
export function useAppBootstrap() {
  React.useEffect(() => {
    installErrorLogging();
    invoke<"mica" | "vibrancy" | "none">("window_material")
      .then((material) => document.documentElement.setAttribute("data-material", material))
      .catch(() => document.documentElement.setAttribute("data-material", "none"));
    applyZoom(useUiStore.getState().zoom);
    // Side panel tabs, pages and terminals follow the chat showing.
    startChatScope();
    // Split view follows the active chat and the chat list (chat-split-view FR-4, FR-12).
    startSplit();
    // The shell's notices about chats no window shows reach this window's bell.
    if (inTauri) startShellNotices(getCurrentWindow().label);
    // The main window follows app updates and raises their notifications.
    if (chatWindowId === null) void startUpdates();
    // Scheduled tasks run in the shell, with or without a window
    // (docs/features/shell-hosted-engines/spec.md, FR-7).
    void useEngineStore.getState().connect();
  }, []);
}

/** Show the terminal (a new one with `fresh`), or hide the side panel when the terminal has focus. */
function toggleTerminal(fresh: boolean) {
  const ui = useUiStore.getState();
  const terminals = useTerminalStore.getState();
  const shown = ui.rightPanelOpen && ui.activePanel === "terminal";
  if (fresh) {
    const cwd = useEngineStore.getState().cwd;
    ui.openPanel("terminal");
    // With none open yet, the first one is the new one.
    void terminals.ensure(cwd).then((created) => {
      if (!created) void useTerminalStore.getState().create({ cwd });
    });
    return;
  }
  if (shown && document.activeElement?.closest(".xterm")) {
    ui.toggleRightPanel(false);
    return;
  }
  terminals.requestFocus();
  ui.openPanel("terminal");
}

/** Global shortcuts with the platform's primary modifier. */
export function useAppHotkeys() {
  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // Ctrl+Tab / Ctrl+Shift+Tab cycle the side panel's tabs (Ctrl on every
      // platform, as in browsers).
      if (event.ctrlKey && event.key === "Tab" && !event.altKey) {
        const ui = useUiStore.getState();
        if (ui.rightPanelOpen && ui.openTabs.length > 0) {
          event.preventDefault();
          ui.cycleTab(event.shiftKey ? -1 : 1);
        }
        return;
      }
      // Terminal, with Ctrl on every platform (⌘` switches windows on macOS):
      // Ctrl+` shows it (or hides the side panel from inside it), Ctrl+Shift+` starts another.
      if (event.ctrlKey && !event.altKey && !event.metaKey && event.code === "Backquote") {
        event.preventDefault();
        toggleTerminal(event.shiftKey);
        return;
      }
      const history = useHistoryStore.getState();
      // Back / forward: Alt+←/→, or ⌘[ / ⌘] on macOS.
      if (!isMac && event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          event.preventDefault();
          if (event.key === "ArrowLeft") history.back();
          else history.forward();
        }
        return;
      }
      if (event.key === "F11" && !isMac) {
        event.preventDefault();
        void toggleFullscreen();
        return;
      }
      if (isMac && event.metaKey && event.ctrlKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        void toggleFullscreen();
        return;
      }
      if (!hasMod(event)) return;
      const key = event.key.toLowerCase();
      const ui = useUiStore.getState();
      const engine = useEngineStore.getState();

      // Developer tools: Ctrl+Shift+I, or ⌥⌘I on macOS.
      if (key === "i" && (isMac ? event.altKey && !event.shiftKey : event.shiftKey && !event.altKey)) {
        event.preventDefault();
        toggleDevtools(true);
        return;
      }
      if (event.altKey) return;

      if (isMac && (event.key === "[" || event.key === "]")) {
        event.preventDefault();
        if (event.key === "[") history.back();
        else history.forward();
      } else if (key === "=" || key === "+") {
        event.preventDefault();
        zoomBy(1);
      } else if (key === "-" && !event.shiftKey) {
        event.preventDefault();
        zoomBy(-1);
      } else if (key === "0" && !event.shiftKey) {
        event.preventDefault();
        resetZoom();
      } else if (key === "r" && !event.shiftKey) {
        event.preventDefault();
        reloadWindow();
      } else if (key === "p" && !event.shiftKey) {
        event.preventDefault();
        ui.setCommandOpen(!ui.commandOpen);
      } else if (key === "n" && !event.shiftKey) {
        event.preventDefault();
        void engine.newChat();
      } else if (key === "o") {
        event.preventDefault();
        void open({ directory: true, multiple: false }).then((folder) => {
          if (typeof folder === "string") void engine.openWorkspace(folder);
        });
      } else if (key === ",") {
        event.preventDefault();
        if (ui.settingsOpen) ui.closeSettings();
        else ui.openSettings();
      } else if (event.key === "/" && !event.shiftKey) {
        // Every shortcut, in one list (ux-polish FR-7).
        event.preventDefault();
        ui.setShortcutsOpen(!ui.shortcutsOpen);
      } else if (key === "j") {
        event.preventDefault();
        ui.toggleRightPanel();
      } else if ((key === "1" || key === "2") && !event.shiftKey && useSplitStore.getState().panes) {
        // Split view: focus the left or right pane (chat-split-view FR-8).
        event.preventDefault();
        useSplitStore.getState().focusIndex(key === "1" ? 0 : 1);
      } else if (event.code === "Backslash" && !event.shiftKey && useSplitStore.getState().panes) {
        // Split view: close the focused pane; its chat goes on.
        event.preventDefault();
        const split = useSplitStore.getState();
        split.close(split.focus);
      } else if (!event.shiftKey) {
        // Tool panels: Ctrl/⌘ + key (see the panel registry).
        const panel = panels.find((p) => p.shortcut.toLowerCase() === key);
        if (panel) {
          event.preventDefault();
          ui.openPanel(panel.id);
        }
      }
    };
    // Mouse back / forward buttons.
    const onMouseUp = (event: MouseEvent) => {
      if (event.button === 3) useHistoryStore.getState().back();
      else if (event.button === 4) useHistoryStore.getState().forward();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);
}
