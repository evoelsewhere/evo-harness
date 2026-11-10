// What clicking a notification does: go to its target and mark it read. And
// what its buttons do: run a command a feature registered (FR-14).

import { openLink } from "@/features/shell/app-commands";
import { useEngineStore } from "@/stores/engine-store";
import { useNotificationStore, type AppNotification, type NotificationAction } from "@/stores/notification-store";
import { useSessionStore } from "@/stores/session-store";
import { useUiStore } from "@/stores/ui-store";

export function openNotification(notification: AppNotification) {
  const store = useNotificationStore.getState();
  store.markRead(notification.id);
  store.dismissToast(notification.id);
  const target = notification.target;
  if (!target) return;
  const ui = useUiStore.getState();
  switch (target.type) {
    case "session":
      if (ui.settingsOpen) ui.closeSettings();
      if (useSessionStore.getState().sessions.some((s) => s.id === target.id)) {
        void useEngineStore.getState().selectSession(target.id);
      }
      break;
    case "panel":
      if (ui.settingsOpen) ui.closeSettings();
      ui.openPanel(target.id);
      break;
    case "settings":
      ui.openSettings(target.section);
      break;
    case "customize":
      ui.openCustomize(target.section);
      break;
    case "url":
      openLink(target.href);
      break;
  }
}

const commands = new Map<string, () => void>();

/** Let notifications carry a button that runs `run` ("updates.install", …). */
export function registerNotificationCommand(command: string, run: () => void) {
  commands.set(command, run);
}

/** The buttons this app version can run; others (a removed command) are hidden. */
export const runnableActions = (notification: AppNotification) =>
  (notification.actions ?? []).filter((action) => commands.has(action.command));

export function runNotificationAction(notification: AppNotification, action: NotificationAction) {
  const store = useNotificationStore.getState();
  store.markRead(notification.id);
  store.dismissToast(notification.id);
  commands.get(action.command)?.();
}
